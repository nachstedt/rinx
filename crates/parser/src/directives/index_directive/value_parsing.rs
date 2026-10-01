use rinx_ast::{IndexEntry, IndexEntryType, InvalidIndexEntry};

/// Strips a leading `!` (the "main entry" marker) from a trimmed entry.
///
/// Returns the remaining text and whether the marker was present. Must only
/// be applied to a whole entry (before type-keyword detection) — never to a
/// typed entry's value, since a real index value can legitimately start with
/// a literal `!` character (e.g. `single: ! (exclamation); in formatted
/// string literal`).
pub(crate) fn strip_main_prefix(entry: &str) -> (&str, bool) {
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

/// Splits a `see:`/`seealso:` value of the form `entry; other` at its
/// first `;`, as Sphinx's `_split_into(2, 'see', value)` does — so a later
/// `;` belongs to `other`.
///
/// Returns `None` unless both parts are non-empty.
fn parse_see_value(value: &str) -> Option<(String, String)> {
    let (entry, target) = value.split_once(';')?;
    let (entry, target) = (entry.trim(), target.trim());
    if entry.is_empty() || target.is_empty() {
        return None;
    }
    Some((entry.to_string(), target.to_string()))
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

/// Builds the `IndexEntry`s for an entry written as `entry_type`, its raw
/// value text, and whether the whole entry was `!`-marked as main.
///
/// # Errors
///
/// A value its type cannot split — an empty `single:`, a `pair:` without
/// two parts — is returned as an [`InvalidIndexEntry`], which the
/// `.. index::` directive and the `:index:` role each report under their own
/// code.
pub(crate) fn parse_typed_entry(
    entry_type: IndexEntryType,
    value: &str,
    main: bool,
) -> Result<Vec<IndexEntry>, InvalidIndexEntry> {
    let entries = match entry_type {
        IndexEntryType::Single => (!value.trim().is_empty()).then(|| {
            let (primary, subentry) = parse_single_value(value);
            vec![IndexEntry::Term {
                primary,
                subentry,
                main,
            }]
        }),
        IndexEntryType::Pair => parse_pair_value(value).map(|(a, b)| expand_pair(&a, &b, main)),
        IndexEntryType::Triple => {
            parse_triple_value(value).map(|(a, b, c)| expand_triple(&a, &b, &c, main))
        }
        IndexEntryType::See => {
            parse_see_value(value).map(|(entry, target)| vec![IndexEntry::See { entry, target }])
        }
        IndexEntryType::SeeAlso => parse_see_value(value)
            .map(|(entry, target)| vec![IndexEntry::SeeAlso { entry, target }]),
    };
    entries.ok_or_else(|| InvalidIndexEntry {
        entry_type,
        value: value.to_string(),
    })
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
    fn test_parse_see_value_splits_entry_and_target() {
        // Given
        let value = "Python; environment";

        // When
        let result = parse_see_value(value);

        // Then
        assert_eq!(
            result,
            Some(("Python".to_string(), "environment".to_string()))
        );
    }

    #[test]
    fn test_parse_see_value_keeps_a_later_semicolon_in_the_target() {
        // Given — Sphinx splits at the first `;` only
        let value = "a; b; c";

        // When
        let result = parse_see_value(value);

        // Then
        assert_eq!(result, Some(("a".to_string(), "b; c".to_string())));
    }

    #[test]
    fn test_parse_see_value_rejects_a_value_without_two_parts() {
        // Given / When / Then
        assert_eq!(parse_see_value("Python"), None);
        assert_eq!(parse_see_value("Python;"), None);
        assert_eq!(parse_see_value("; environment"), None);
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
    fn test_parse_typed_entry_builds_a_single_term() {
        // Given / When
        let entries = parse_typed_entry(IndexEntryType::Single, "execution; context", true);

        // Then
        assert_eq!(
            entries,
            Ok(vec![IndexEntry::Term {
                primary: "execution".to_string(),
                subentry: Some("context".to_string()),
                main: true,
            }])
        );
    }

    #[test]
    fn test_parse_typed_entry_refuses_an_empty_single() {
        // Given / When
        let result = parse_typed_entry(IndexEntryType::Single, "  ", false);

        // Then
        assert_eq!(
            result,
            Err(InvalidIndexEntry {
                entry_type: IndexEntryType::Single,
                value: "  ".to_string(),
            })
        );
    }

    #[test]
    fn test_parse_typed_entry_expands_a_pair() {
        // Given / When
        let entries = parse_typed_entry(IndexEntryType::Pair, "loop; statement", false);

        // Then
        assert_eq!(entries.map(|entries| entries.len()), Ok(2));
    }

    #[test]
    fn test_parse_typed_entry_refuses_each_malformed_type_under_its_own_type() {
        // Given
        let cases = [
            (IndexEntryType::Pair, "loop"),
            (IndexEntryType::Triple, "a; b"),
            (IndexEntryType::See, "a"),
            (IndexEntryType::SeeAlso, "a"),
        ];

        for (entry_type, value) in cases {
            // When
            let result = parse_typed_entry(entry_type, value, false);

            // Then
            assert_eq!(
                result,
                Err(InvalidIndexEntry {
                    entry_type,
                    value: value.to_string(),
                }),
                "{entry_type:?}"
            );
        }
    }

    #[test]
    fn test_parse_typed_entry_builds_see_and_seealso_redirects() {
        // Given / When
        let see = parse_typed_entry(IndexEntryType::See, "goto; jump", false);
        let see_also = parse_typed_entry(IndexEntryType::SeeAlso, "goto; jump", false);

        // Then
        assert_eq!(
            see,
            Ok(vec![IndexEntry::See {
                entry: "goto".to_string(),
                target: "jump".to_string(),
            }])
        );
        assert_eq!(
            see_also,
            Ok(vec![IndexEntry::SeeAlso {
                entry: "goto".to_string(),
                target: "jump".to_string(),
            }])
        );
    }
}
