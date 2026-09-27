use crate::diagnostics::Diagnostics;
use rinx_ast::{Diagnostic, DiagnosticCode, IndexEntry, Span};

/// Strips a leading `!` (the "main entry" marker) from a trimmed entry.
///
/// Returns the remaining text and whether the marker was present. Must only
/// be applied to a whole entry (before type-keyword detection) — never to a
/// typed entry's value, since a real index value can legitimately start with
/// a literal `!` character (e.g. `single: ! (exclamation); in formatted
/// string literal`).
pub(super) fn strip_main_prefix(entry: &str) -> (&str, bool) {
    entry
        .strip_prefix('!')
        .map_or((entry, false), |rest| (rest.trim_start(), true))
}

/// Splits a `single:` value into its primary term and an optional subentry,
/// on the first `;` — but only if *both* resulting parts are non-empty
/// after trimming (mirrors real Sphinx's `_split_into`, which falls back to
/// treating the whole value as one literal, unsplit primary otherwise).
/// Without this check, a value that legitimately starts with a literal `;`
/// character (real `CPython` usage: `single: ; (semicolon)`,
/// `Doc/library/os.rst`) would split into a bogus empty primary and a
/// `"(semicolon)"` subentry instead of indexing the semicolon itself.
fn parse_single_value(value: &str) -> (String, Option<String>) {
    if let Some((primary, subentry)) = value.split_once(';') {
        let primary = primary.trim();
        let subentry = subentry.trim();
        if !primary.is_empty() && !subentry.is_empty() {
            return (primary.to_string(), Some(subentry.to_string()));
        }
    }
    (value.trim().to_string(), None)
}

/// Splits a `pair:` value into its two `;`-separated parts.
///
/// Returns `None` if the value doesn't contain exactly two non-empty parts.
fn parse_pair_value(value: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = value.split(';').map(str::trim).collect();
    match parts.as_slice() {
        [a, b] if !a.is_empty() && !b.is_empty() => Some(((*a).to_string(), (*b).to_string())),
        _ => None,
    }
}

/// Splits a `triple:` value into its three `;`-separated parts.
///
/// Returns `None` if the value doesn't contain exactly three non-empty parts.
fn parse_triple_value(value: &str) -> Option<(String, String, String)> {
    let parts: Vec<&str> = value.split(';').map(str::trim).collect();
    match parts.as_slice() {
        [a, b, c] if !a.is_empty() && !b.is_empty() && !c.is_empty() => {
            Some(((*a).to_string(), (*b).to_string(), (*c).to_string()))
        }
        _ => None,
    }
}

/// Splits a `see:`/`seealso:` value of the form `entry <target>`.
///
/// Returns `None` if no `<...>` target is present.
fn parse_target_value(value: &str) -> Option<(String, String)> {
    let angle_start = value.rfind('<')?;
    let angle_end = value[angle_start..].find('>')?;
    let entry = value[..angle_start].trim().to_string();
    let target = value[angle_start + 1..angle_start + angle_end]
        .trim()
        .to_string();
    if entry.is_empty() || target.is_empty() {
        return None;
    }
    Some((entry, target))
}

/// Expands a `pair: A; B` value into its two reciprocal `single`-equivalent
/// entries: "A; B" and "B; A".
fn expand_pair(a: &str, b: &str, main: bool) -> Vec<IndexEntry> {
    vec![
        IndexEntry::Term {
            primary: a.to_string(),
            subentry: Some(b.to_string()),
            main,
        },
        IndexEntry::Term {
            primary: b.to_string(),
            subentry: Some(a.to_string()),
            main,
        },
    ]
}

/// Expands a `triple: A; B; C` value into three entries, each pairing one
/// value against the other two (space-joined subentry text, a simplification
/// of Sphinx's exact display string — see `docs/compatibility.rst`).
fn expand_triple(a: &str, b: &str, c: &str, main: bool) -> Vec<IndexEntry> {
    vec![
        IndexEntry::Term {
            primary: a.to_string(),
            subentry: Some(format!("{b} {c}")),
            main,
        },
        IndexEntry::Term {
            primary: b.to_string(),
            subentry: Some(format!("{c} {a}")),
            main,
        },
        IndexEntry::Term {
            primary: c.to_string(),
            subentry: Some(format!("{a} {b}")),
            main,
        },
    ]
}

/// One of the five recognized `.. index::` entry-type keywords.
pub(super) fn is_known_entry_type(entry_type: &str) -> bool {
    matches!(entry_type, "single" | "pair" | "triple" | "see" | "seealso")
}

/// Builds the `IndexEntry`s for a known `entry_type` (`single`/`pair`/
/// `triple`/`see`/`seealso`), its raw value text, and whether the whole
/// entry was `!`-marked as main. `original` is the unparsed source (line or
/// comma-segment) used in diagnostics. Malformed values push a diagnostic
/// and produce no entries, staying error-resilient.
pub(super) fn parse_typed_entry(
    entry_type: &str,
    value: &str,
    main: bool,
    original: &str,
    diagnostics: &mut Diagnostics,
    span: Option<Span>,
) -> Vec<IndexEntry> {
    match entry_type {
        "single" => {
            let (primary, subentry) = parse_single_value(value);
            vec![IndexEntry::Term {
                primary,
                subentry,
                main,
            }]
        }
        "pair" => parse_pair_value(value).map_or_else(
            || {
                diagnostics.push(Diagnostic::at(
                    DiagnosticCode::IndexInvalidPair,
                    format!("Invalid .. index:: pair entry: {original}"),
                    span,
                ));
                Vec::new()
            },
            |(a, b)| expand_pair(&a, &b, main),
        ),
        "triple" => parse_triple_value(value).map_or_else(
            || {
                diagnostics.push(Diagnostic::at(
                    DiagnosticCode::IndexInvalidTriple,
                    format!("Invalid .. index:: triple entry: {original}"),
                    span,
                ));
                Vec::new()
            },
            |(a, b, c)| expand_triple(&a, &b, &c, main),
        ),
        "see" => parse_target_value(value).map_or_else(
            || {
                diagnostics.push(Diagnostic::at(
                    DiagnosticCode::IndexInvalidSee,
                    format!("Invalid .. index:: see entry: {original}"),
                    span,
                ));
                Vec::new()
            },
            |(entry, target)| vec![IndexEntry::See { entry, target }],
        ),
        "seealso" => parse_target_value(value).map_or_else(
            || {
                diagnostics.push(Diagnostic::at(
                    DiagnosticCode::IndexInvalidSeeAlso,
                    format!("Invalid .. index:: seealso entry: {original}"),
                    span,
                ));
                Vec::new()
            },
            |(entry, target)| vec![IndexEntry::SeeAlso { entry, target }],
        ),
        _ => unreachable!("caller must guard with is_known_entry_type"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_main_prefix_strips_leading_bang() {
        // Given
        let entry = "!single: Python";

        // When
        let (stripped, main) = strip_main_prefix(entry);

        // Then
        assert_eq!(stripped, "single: Python");
        assert!(main);
    }

    #[test]
    fn test_strip_main_prefix_leaves_entry_unchanged_without_bang() {
        // Given
        let entry = "single: Python";

        // When
        let (stripped, main) = strip_main_prefix(entry);

        // Then
        assert_eq!(stripped, "single: Python");
        assert!(!main);
    }

    #[test]
    fn test_parse_single_value_without_subentry() {
        // Given
        let value = "execution";

        // When
        let (primary, subentry) = parse_single_value(value);

        // Then
        assert_eq!(primary, "execution");
        assert_eq!(subentry, None);
    }

    #[test]
    fn test_parse_single_value_with_subentry() {
        // Given
        let value = "execution; context";

        // When
        let (primary, subentry) = parse_single_value(value);

        // Then
        assert_eq!(primary, "execution");
        assert_eq!(subentry, Some("context".to_string()));
    }

    #[test]
    fn test_parse_single_value_preserves_literal_bang_in_value() {
        // Given — real Sphinx: `single: ! (exclamation); in formatted
        // string literal` indexes a literal "!" character, not a main-entry
        // marker (the marker, if present, would already have been stripped
        // from the *whole entry* before this point).
        let value = "! (exclamation); in formatted string literal";

        // When
        let (primary, subentry) = parse_single_value(value);

        // Then
        assert_eq!(primary, "! (exclamation)");
        assert_eq!(subentry, Some("in formatted string literal".to_string()));
    }

    #[test]
    fn test_parse_single_value_leaves_leading_semicolon_unsplit() {
        // Given — real CPython usage (Doc/library/os.rst): `single: ;
        // (semicolon)` indexes the literal semicolon character. Splitting
        // on the first `;` would produce a bogus empty primary — real
        // Sphinx's `_split_into` requires *both* resulting parts to be
        // non-empty, falling back to the whole value as one unsplit
        // primary otherwise.
        let value = "; (semicolon)";

        // When
        let (primary, subentry) = parse_single_value(value);

        // Then
        assert_eq!(primary, "; (semicolon)");
        assert_eq!(subentry, None);
    }

    #[test]
    fn test_parse_single_value_leaves_trailing_semicolon_unsplit() {
        // Given — a trailing `;` with no subentry text after it would also
        // produce an empty part on the subentry side; same fallback applies.
        let value = "foo;";

        // When
        let (primary, subentry) = parse_single_value(value);

        // Then
        assert_eq!(primary, "foo;");
        assert_eq!(subentry, None);
    }

    #[test]
    fn test_parse_pair_value_splits_two_parts() {
        // Given
        let value = "loop; statement";

        // When
        let result = parse_pair_value(value);

        // Then
        assert_eq!(result, Some(("loop".to_string(), "statement".to_string())));
    }

    #[test]
    fn test_parse_pair_value_rejects_wrong_part_count() {
        // Given
        let value = "loop; statement; extra";

        // When
        let result = parse_pair_value(value);

        // Then
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_triple_value_splits_three_parts() {
        // Given
        let value = "A; B; C";

        // When
        let result = parse_triple_value(value);

        // Then
        assert_eq!(
            result,
            Some(("A".to_string(), "B".to_string(), "C".to_string()))
        );
    }

    #[test]
    fn test_parse_triple_value_rejects_wrong_part_count() {
        // Given
        let value = "A; B";

        // When
        let result = parse_triple_value(value);

        // Then
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_target_value_splits_entry_and_target() {
        // Given
        let value = "Python <environment>";

        // When
        let result = parse_target_value(value);

        // Then
        assert_eq!(
            result,
            Some(("Python".to_string(), "environment".to_string()))
        );
    }

    #[test]
    fn test_parse_target_value_rejects_missing_angle_brackets() {
        // Given
        let value = "Python";

        // When
        let result = parse_target_value(value);

        // Then
        assert_eq!(result, None);
    }

    #[test]
    fn test_expand_pair_produces_two_reciprocal_entries() {
        // Given
        let (a, b) = ("loop", "statement");

        // When
        let entries = expand_pair(a, b, false);

        // Then
        assert_eq!(
            entries,
            vec![
                IndexEntry::Term {
                    primary: "loop".to_string(),
                    subentry: Some("statement".to_string()),
                    main: false,
                },
                IndexEntry::Term {
                    primary: "statement".to_string(),
                    subentry: Some("loop".to_string()),
                    main: false,
                },
            ]
        );
    }

    #[test]
    fn test_expand_triple_produces_three_permutation_entries() {
        // Given
        let (a, b, c) = ("A", "B", "C");

        // When
        let entries = expand_triple(a, b, c, false);

        // Then
        assert_eq!(
            entries,
            vec![
                IndexEntry::Term {
                    primary: "A".to_string(),
                    subentry: Some("B C".to_string()),
                    main: false,
                },
                IndexEntry::Term {
                    primary: "B".to_string(),
                    subentry: Some("C A".to_string()),
                    main: false,
                },
                IndexEntry::Term {
                    primary: "C".to_string(),
                    subentry: Some("A B".to_string()),
                    main: false,
                },
            ]
        );
    }

    #[test]
    fn test_is_known_entry_type_accepts_all_five_keywords() {
        // Given / When / Then
        for keyword in ["single", "pair", "triple", "see", "seealso"] {
            assert!(is_known_entry_type(keyword), "{keyword} should be known");
        }
    }

    #[test]
    fn test_is_known_entry_type_rejects_unrecognized_keyword() {
        // Given / When / Then
        assert!(!is_known_entry_type("bogus"));
    }
}
