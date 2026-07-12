//! `.. index::` directive parsing for RST documents.
//!
//! `pair:`/`triple:` entries are expanded into plain `single:`-equivalent
//! `IndexEntry::Term`s here, at parse time, mirroring how real Sphinx treats
//! them as pure shorthand — downstream code (analyzer, renderer) therefore
//! only ever has to handle one linkable entry shape.
//!
//! The argument line and every body line share one grammar (confirmed
//! against real-world usage in `CPython`'s own docs): each **physical line**
//! is checked, as a whole, against the known type keywords first
//! (`single:`, `pair:`, `triple:`, `see:`, `seealso:`, optionally
//! `!`-prefixed to mark the entry main). If a type keyword matches,
//! everything after it is *one* value — including any literal `,` it
//! contains, e.g. `single: , (comma); in string formatting`
//! (`Doc/library/string.rst`). Only when *no* type keyword matches does the
//! line fall back to being a comma-separated list of bare shorthand terms
//! (`BNF, grammar, syntax`), and even then a term may itself contain a
//! literal `;`, e.g. `object; code, code object` (`Doc/c-api/code.rst`) —
//! bare terms never get subentry-split. A line is never partly typed and
//! partly bare-shorthand. The `!` main-entry marker is strictly a
//! whole-line prefix *before* the type keyword — never embedded in the
//! value — since real Sphinx entries like `single: ! (exclamation); in
//! formatted string literal` use a literal `!` character as part of the
//! indexed text itself.
//!
//! `single:`'s value/subentry split on `;` (`parse_single_value`) has the
//! same "don't split on a delimiter that's part of the literal indexed
//! text" concern, one level down: `single: ; (semicolon)`
//! (`Doc/library/os.rst`) indexes a literal semicolon, so splitting
//! unconditionally on the first `;` would produce a bogus empty primary. It
//! only splits when *both* resulting parts are non-empty, mirroring real
//! Sphinx's `sphinx.util.index_entries._split_into`.

use super::bullet_list::unindent_body_lines;
use rusty_sphinx_ast::IndexEntry;

/// Strips a leading `!` (the "main entry" marker) from a trimmed entry.
///
/// Returns the remaining text and whether the marker was present. Must only
/// be applied to a whole entry (before type-keyword detection) — never to a
/// typed entry's value, since a real index value can legitimately start with
/// a literal `!` character (e.g. `single: ! (exclamation); in formatted
/// string literal`).
fn strip_main_prefix(entry: &str) -> (&str, bool) {
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
/// of Sphinx's exact display string — see `spec_gaps.md`).
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
fn is_known_entry_type(entry_type: &str) -> bool {
    matches!(entry_type, "single" | "pair" | "triple" | "see" | "seealso")
}

/// Builds the `IndexEntry`s for a known `entry_type` (`single`/`pair`/
/// `triple`/`see`/`seealso`), its raw value text, and whether the whole
/// entry was `!`-marked as main. `original` is the unparsed source (line or
/// comma-segment) used in diagnostics. Malformed values push a diagnostic
/// and produce no entries, staying error-resilient.
fn parse_typed_entry(
    entry_type: &str,
    value: &str,
    main: bool,
    original: &str,
    diagnostics: &mut Vec<String>,
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
                diagnostics.push(format!("Invalid .. index:: pair entry: {original}"));
                Vec::new()
            },
            |(a, b)| expand_pair(&a, &b, main),
        ),
        "triple" => parse_triple_value(value).map_or_else(
            || {
                diagnostics.push(format!("Invalid .. index:: triple entry: {original}"));
                Vec::new()
            },
            |(a, b, c)| expand_triple(&a, &b, &c, main),
        ),
        "see" => parse_target_value(value).map_or_else(
            || {
                diagnostics.push(format!("Invalid .. index:: see entry: {original}"));
                Vec::new()
            },
            |(entry, target)| vec![IndexEntry::See { entry, target }],
        ),
        "seealso" => parse_target_value(value).map_or_else(
            || {
                diagnostics.push(format!("Invalid .. index:: seealso entry: {original}"));
                Vec::new()
            },
            |(entry, target)| vec![IndexEntry::SeeAlso { entry, target }],
        ),
        _ => unreachable!("caller must guard with is_known_entry_type"),
    }
}

/// Parses one physical line — either the directive's argument line or a
/// single body line, since both share the same grammar. A leading `!`
/// (before any type keyword) marks the whole line as main; what remains is
/// checked *as a whole* against the known type keywords first, so a typed
/// value's own literal commas (`single: , (comma); in string formatting`)
/// are never mistaken for term separators. Only if no type keyword matches
/// does the line fall back to being a comma-separated list of bare
/// shorthand terms (`BNF`, or a continuation line like `__cause__
/// (exception attribute)`, or `object; code, code object` where a bare term
/// itself contains a literal `;`).
fn parse_index_line(line: &str, diagnostics: &mut Vec<String>) -> Vec<IndexEntry> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let (rest, main) = strip_main_prefix(trimmed);
    let rest = rest.trim();

    if let Some((entry_type, raw_value)) = rest.split_once(':') {
        let entry_type = entry_type.trim();
        if is_known_entry_type(entry_type) {
            return parse_typed_entry(entry_type, raw_value.trim(), main, line, diagnostics);
        }
    }

    rest.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|term| IndexEntry::Term {
            primary: term.to_string(),
            subentry: None,
            main,
        })
        .collect()
}

/// Parses a `.. index::` directive's argument and body into its entries.
///
/// The argument line and each body line are parsed identically (see the
/// module docs) — the two forms can be freely combined, matching how real
/// Sphinx documents write a first entry on the directive line and further
/// entries as continuation lines below it.
pub(super) fn parse_index_entries(
    argument: &str,
    body_lines: &[&str],
    diagnostics: &mut Vec<String>,
) -> Vec<IndexEntry> {
    let mut entries = parse_index_line(argument, diagnostics);

    let unindented = unindent_body_lines(body_lines);
    for line in &unindented {
        entries.extend(parse_index_line(line, diagnostics));
    }

    entries
}

/// Parses a `.. index::` directive into a `Directive::Index`. The anchor
/// `id` starts empty here — it's assigned by the post-parse
/// `assign_index_ids` pass, since a bare-location directive like this has no
/// content-derived identity to build one from at parse time.
pub(super) fn parse_index_directive(
    argument: &str,
    body_lines: &[&str],
    diagnostics: &mut Vec<String>,
) -> rusty_sphinx_ast::Directive {
    rusty_sphinx_ast::Directive::Index {
        entries: parse_index_entries(argument, body_lines, diagnostics),
        id: String::new(),
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

    #[test]
    fn test_parse_index_line_dispatches_typed_entry() {
        // Given — the single-line form `.. index:: single: execution`
        let line = "single: execution";
        let mut diagnostics = Vec::new();

        // When
        let entries = parse_index_line(line, &mut diagnostics);

        // Then
        assert_eq!(
            entries,
            vec![IndexEntry::Term {
                primary: "execution".to_string(),
                subentry: None,
                main: false,
            }]
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_index_line_dispatches_typed_pair_entry() {
        // Given
        let line = "pair: loop; statement";
        let mut diagnostics = Vec::new();

        // When
        let entries = parse_index_line(line, &mut diagnostics);

        // Then
        assert_eq!(entries.len(), 2);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_index_line_treats_untyped_line_as_bare_term() {
        // Given — the comma-shorthand form has no colon at all
        let line = "BNF";
        let mut diagnostics = Vec::new();

        // When
        let entries = parse_index_line(line, &mut diagnostics);

        // Then
        assert_eq!(
            entries,
            vec![IndexEntry::Term {
                primary: "BNF".to_string(),
                subentry: None,
                main: false,
            }]
        );
    }

    #[test]
    fn test_parse_index_line_treats_unrecognized_colon_prefix_as_bare_term() {
        // Given — a colon that isn't one of the five known type keywords
        // should not be mistaken for a type prefix
        let line = "Section 3: Advanced Topics";
        let mut diagnostics = Vec::new();

        // When
        let entries = parse_index_line(line, &mut diagnostics);

        // Then
        assert_eq!(
            entries,
            vec![IndexEntry::Term {
                primary: "Section 3: Advanced Topics".to_string(),
                subentry: None,
                main: false,
            }]
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_index_line_bang_before_type_keyword_marks_main() {
        // Given — real CPython usage: `! pair: statement; if`
        let line = "! pair: statement; if";
        let mut diagnostics = Vec::new();

        // When
        let entries = parse_index_line(line, &mut diagnostics);

        // Then — both expanded entries are marked main
        assert_eq!(entries.len(), 2);
        assert!(
            entries
                .iter()
                .all(|e| matches!(e, IndexEntry::Term { main: true, .. }))
        );
    }

    #[test]
    fn test_parse_index_line_bang_before_single_type_keyword_marks_main() {
        // Given — real CPython usage: `! single: pattern matching`
        let line = "! single: pattern matching";
        let mut diagnostics = Vec::new();

        // When
        let entries = parse_index_line(line, &mut diagnostics);

        // Then
        assert_eq!(
            entries,
            vec![IndexEntry::Term {
                primary: "pattern matching".to_string(),
                subentry: None,
                main: true,
            }]
        );
    }

    #[test]
    fn test_parse_index_line_preserves_literal_bang_inside_value() {
        // Given — real CPython usage: `single: ! (exclamation); in glob-style
        // wildcards` — the "!" here is literal indexed text, not a marker,
        // since it comes after the type keyword, not before it.
        let line = "single: ! (exclamation); in glob-style wildcards";
        let mut diagnostics = Vec::new();

        // When
        let entries = parse_index_line(line, &mut diagnostics);

        // Then
        assert_eq!(
            entries,
            vec![IndexEntry::Term {
                primary: "! (exclamation)".to_string(),
                subentry: Some("in glob-style wildcards".to_string()),
                main: false,
            }]
        );
    }

    #[test]
    fn test_parse_index_line_typed_single_preserves_literal_comma_in_value() {
        // Given — real CPython usage (Doc/library/string.rst:453): a
        // `single:` entry that indexes the literal comma character. Splitting
        // on `,` before recognizing the `single:` type keyword would cut this
        // value in half and produce a bogus empty-primary entry instead.
        let line = "single: , (comma); in string formatting";
        let mut diagnostics = Vec::new();

        // When
        let entries = parse_index_line(line, &mut diagnostics);

        // Then
        assert_eq!(
            entries,
            vec![IndexEntry::Term {
                primary: ", (comma)".to_string(),
                subentry: Some("in string formatting".to_string()),
                main: false,
            }]
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_index_line_bare_shorthand_term_may_contain_semicolon() {
        // Given — a fragment of real CPython usage (Doc/c-api/code.rst:3):
        // an untyped bare term never gets subentry-split on `;`.
        let line = "object; code";
        let mut diagnostics = Vec::new();

        // When
        let entries = parse_index_line(line, &mut diagnostics);

        // Then
        assert_eq!(
            entries,
            vec![IndexEntry::Term {
                primary: "object; code".to_string(),
                subentry: None,
                main: false,
            }]
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_index_line_bare_comma_shorthand_with_semicolon_in_first_term() {
        // Given — real CPython usage (Doc/c-api/code.rst:3): a comma
        // separates two bare terms, the first of which itself contains a
        // literal `;`.
        let line = "object; code, code object";
        let mut diagnostics = Vec::new();

        // When
        let entries = parse_index_line(line, &mut diagnostics);

        // Then
        assert_eq!(
            entries,
            vec![
                IndexEntry::Term {
                    primary: "object; code".to_string(),
                    subentry: None,
                    main: false,
                },
                IndexEntry::Term {
                    primary: "code object".to_string(),
                    subentry: None,
                    main: false,
                },
            ]
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_index_line_produces_one_term_per_shorthand_value() {
        // Given
        let line = "BNF, grammar, syntax";
        let mut diagnostics = Vec::new();

        // When
        let entries = parse_index_line(line, &mut diagnostics);

        // Then
        assert_eq!(
            entries,
            vec![
                IndexEntry::Term {
                    primary: "BNF".to_string(),
                    subentry: None,
                    main: false,
                },
                IndexEntry::Term {
                    primary: "grammar".to_string(),
                    subentry: None,
                    main: false,
                },
                IndexEntry::Term {
                    primary: "syntax".to_string(),
                    subentry: None,
                    main: false,
                },
            ]
        );
    }

    #[test]
    fn test_parse_index_line_returns_empty_for_blank_line() {
        // Given
        let line = "";
        let mut diagnostics = Vec::new();

        // When
        let entries = parse_index_line(line, &mut diagnostics);

        // Then
        assert!(entries.is_empty());
    }

    #[test]
    fn test_parse_index_line_parses_single_line_typed_form() {
        // Given — the common single-line `.. index:: single: execution` form,
        // with no indented body block at all
        let line = "single: execution";
        let mut diagnostics = Vec::new();

        // When
        let entries = parse_index_line(line, &mut diagnostics);

        // Then
        assert_eq!(
            entries,
            vec![IndexEntry::Term {
                primary: "execution".to_string(),
                subentry: None,
                main: false,
            }]
        );
    }

    #[test]
    fn test_parse_index_line_unknown_type_emits_diagnostic_free_bare_term() {
        // Given — a colon-looking type keyword that isn't recognized falls
        // back to a bare term rather than erroring, staying error-resilient
        let line = "bogus: foo";
        let mut diagnostics = Vec::new();

        // When
        let entries = parse_index_line(line, &mut diagnostics);

        // Then
        assert_eq!(
            entries,
            vec![IndexEntry::Term {
                primary: "bogus: foo".to_string(),
                subentry: None,
                main: false,
            }]
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_index_entries_combines_shorthand_and_body_lines() {
        // Given: 2 shorthand entries (BNF, grammar) + 1 single + 1 pair (expands to 2)
        let argument = "BNF, grammar";
        let body_lines = vec!["   single: execution", "   pair: loop; statement"];
        let mut diagnostics = Vec::new();

        // When
        let entries = parse_index_entries(argument, &body_lines, &mut diagnostics);

        // Then
        assert_eq!(entries.len(), 5);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_index_entries_treats_untyped_continuation_lines_as_bare_terms() {
        // Given — real CPython usage (Doc/library/exceptions.rst): the first
        // entry on the argument line, followed by untyped continuation
        // lines that are their own bare shorthand terms, not part of the
        // first entry's value.
        let argument = "pair: exception; chaining";
        let body_lines = vec![
            "           __cause__ (exception attribute)",
            "           __context__ (exception attribute)",
        ];
        let mut diagnostics = Vec::new();

        // When
        let entries = parse_index_entries(argument, &body_lines, &mut diagnostics);

        // Then — pair expands to 2, plus 2 bare terms = 4
        assert_eq!(entries.len(), 4);
        assert!(diagnostics.is_empty());
        assert!(entries.iter().any(|e| matches!(
            e,
            IndexEntry::Term { primary, .. } if primary == "__cause__ (exception attribute)"
        )));
    }

    #[test]
    fn test_parse_index_entries_splits_comma_separated_body_line() {
        // Given — real CPython usage (Doc/reference/expressions.rst): a
        // comma-separated list of bare terms on a single body line.
        let argument = "";
        let body_lines = vec!["   key, value, key/value pair"];
        let mut diagnostics = Vec::new();

        // When
        let entries = parse_index_entries(argument, &body_lines, &mut diagnostics);

        // Then
        assert_eq!(entries.len(), 3);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_index_entries_preserves_literal_comma_in_typed_body_line() {
        // Given — real CPython usage (Doc/library/string.rst:452-454): two
        // `single:` body lines, one of which indexes the literal comma
        // character. Regression test for a bug where comma-splitting a line
        // before checking its type keyword corrupted `single: , (comma); ...`
        // into a bogus empty-primary entry (which then sorted first in the
        // rendered index, displaying as the linked page's title).
        let argument = "";
        let body_lines = vec![
            "   single: , (comma); in string formatting",
            "   single: _ (underscore); in string formatting",
        ];
        let mut diagnostics = Vec::new();

        // When
        let entries = parse_index_entries(argument, &body_lines, &mut diagnostics);

        // Then
        assert_eq!(entries.len(), 2);
        assert!(diagnostics.is_empty());
        assert!(entries.iter().any(|e| matches!(
            e,
            IndexEntry::Term { primary, subentry: Some(s), main: false }
                if primary == ", (comma)" && s == "in string formatting"
        )));
        assert!(!entries.iter().any(|e| matches!(
            e,
            IndexEntry::Term { primary, .. } if primary.is_empty()
        )));
    }

    #[test]
    fn test_parse_index_entries_preserves_literal_leading_semicolon_in_typed_body_line() {
        // Given — real CPython usage (Doc/library/os.rst:6314-6315): a
        // `single:` body line that indexes the literal semicolon character.
        // Regression test for a sibling bug to the comma one above: splitting
        // unconditionally on the first `;` corrupted `single: ; (semicolon)`
        // into a bogus empty-primary entry instead of one literal term.
        let argument = "";
        let body_lines = vec![
            "   single: : (colon); path separator (POSIX)",
            "   single: ; (semicolon)",
        ];
        let mut diagnostics = Vec::new();

        // When
        let entries = parse_index_entries(argument, &body_lines, &mut diagnostics);

        // Then
        assert_eq!(entries.len(), 2);
        assert!(diagnostics.is_empty());
        assert!(entries.iter().any(|e| matches!(
            e,
            IndexEntry::Term { primary, subentry: None, main: false }
                if primary == "; (semicolon)"
        )));
        assert!(!entries.iter().any(|e| matches!(
            e,
            IndexEntry::Term { primary, .. } if primary.is_empty()
        )));
    }

    #[test]
    fn test_parse_index_directive_wraps_entries_with_empty_id() {
        // Given
        let argument = "execution";
        let body_lines: Vec<&str> = vec![];
        let mut diagnostics = Vec::new();

        // When
        let directive = parse_index_directive(argument, &body_lines, &mut diagnostics);

        // Then
        if let rusty_sphinx_ast::Directive::Index { entries, id } = directive {
            assert_eq!(entries.len(), 1);
            assert_eq!(id, "");
        } else {
            panic!("Expected Directive::Index");
        }
    }

    #[test]
    fn test_parse_index_entries_empty_argument_and_body() {
        // Given
        let argument = "";
        let body_lines: Vec<&str> = vec![];
        let mut diagnostics = Vec::new();

        // When
        let entries = parse_index_entries(argument, &body_lines, &mut diagnostics);

        // Then
        assert!(entries.is_empty());
        assert!(diagnostics.is_empty());
    }
}
