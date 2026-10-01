//! Turning an `.. index::` directive's argument and body lines into
//! [`rinx_ast::IndexEntry`]s, one physical line at a time.

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::indent::unindent_body_lines;
use rinx_ast::{Diagnostic, DiagnosticCode, IndexEntry, IndexEntryType, InvalidIndexEntry};

use super::value_parsing::{parse_typed_entry, strip_main_prefix};

/// Parses one physical line — either the directive's argument line or a
/// single body line, since both share the same grammar, which is also the
/// grammar of an `:index:` role's explicit target. A leading `!` (before any
/// type keyword) marks the whole line as main; what remains is checked *as a
/// whole* against the known type keywords first, so a typed value's own
/// literal commas (`single: , (comma); in string formatting`) are never
/// mistaken for term separators. Only if no type keyword matches does the
/// line fall back to being a comma-separated list of bare shorthand terms
/// (`BNF`, or a continuation line like `__cause__ (exception attribute)`, or
/// `object; code, code object` where a bare term itself contains a literal
/// `;`), where each term takes its own `!`.
///
/// # Errors
///
/// A typed value its type cannot split, for the caller to report under its
/// own code. A bare shorthand line cannot be malformed.
pub(crate) fn parse_index_line(line: &str) -> Result<Vec<IndexEntry>, InvalidIndexEntry> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let (rest, main) = strip_main_prefix(trimmed);
    let rest = rest.trim();

    if let Some((keyword, raw_value)) = rest.split_once(':')
        && let Some(entry_type) = IndexEntryType::from_keyword(keyword.trim())
    {
        return parse_typed_entry(entry_type, raw_value.trim(), main);
    }

    // Each comma-separated value carries its own `!`, as in Sphinx's
    // `process_index_entry`, which splits the line as written: a leading
    // `!` marks only the first value.
    Ok(trimmed
        .split(',')
        .filter_map(|value| {
            let (term, main) = strip_main_prefix(value.trim());
            (!term.is_empty()).then(|| IndexEntry::Term {
                primary: term.to_string(),
                subentry: None,
                main,
            })
        })
        .collect())
}

/// The code a malformed `.. index::` entry is reported under.
const fn directive_code(entry_type: IndexEntryType) -> DiagnosticCode {
    match entry_type {
        IndexEntryType::Single => DiagnosticCode::IndexInvalidSingle,
        IndexEntryType::Pair => DiagnosticCode::IndexInvalidPair,
        IndexEntryType::Triple => DiagnosticCode::IndexInvalidTriple,
        IndexEntryType::See => DiagnosticCode::IndexInvalidSee,
        IndexEntryType::SeeAlso => DiagnosticCode::IndexInvalidSeeAlso,
    }
}

/// Parses one directive line, reporting a malformed entry at `span` and
/// making no entry for it, so the rest of the directive still indexes.
fn parse_directive_line(
    line: &str,
    diagnostics: &mut Diagnostics,
    span: Option<rinx_ast::Span>,
) -> Vec<IndexEntry> {
    parse_index_line(line).unwrap_or_else(|error| {
        diagnostics.push(Diagnostic::at(
            directive_code(error.entry_type),
            error.to_string(),
            span,
        ));
        Vec::new()
    })
}

/// Parses a `.. index::` directive's argument and body into its entries.
///
/// The argument line and each body line are parsed identically (see the
/// module docs) — the two forms can be freely combined, matching how real
/// Sphinx documents write a first entry on the directive line and further
/// entries as continuation lines below it.
pub(crate) fn parse_index_entries(
    argument: &str,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<IndexEntry> {
    // The argument sits on the directive's marker line, which is above the
    // body `ctx` is positioned at, so it has no span of its own here; the
    // body's own lines do.
    let mut entries = parse_directive_line(argument, diagnostics, None);

    let unindented = unindent_body_lines(body_lines);
    for (index, line) in unindented.iter().enumerate() {
        entries.extend(parse_directive_line(
            line,
            diagnostics,
            ctx.line_span(index, line),
        ));
    }

    entries
}

/// Parses a `.. index::` directive into a `Directive::Index`. The anchor
/// `id` starts empty here — it's assigned by the post-parse
/// `assign_index_ids` pass, since a bare-location directive like this has no
/// content-derived identity to build one from at parse time.
pub(crate) fn parse_index_directive(
    argument: &str,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> rinx_ast::Directive {
    rinx_ast::Directive::Index {
        entries: parse_index_entries(argument, body_lines, diagnostics, ctx),
        id: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::Domain;

    #[test]
    fn test_parse_index_line_dispatches_typed_entry() {
        // Given — the single-line form `.. index:: single: execution`
        let line = "single: execution";

        // When
        let entries = parse_index_line(line).unwrap();

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
    fn test_parse_index_line_dispatches_typed_pair_entry() {
        // Given
        let line = "pair: loop; statement";

        // When
        let entries = parse_index_line(line).unwrap();

        // Then
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_parse_index_line_treats_untyped_line_as_bare_term() {
        // Given — the comma-shorthand form has no colon at all
        let line = "BNF";

        // When
        let entries = parse_index_line(line).unwrap();

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

        // When
        let entries = parse_index_line(line).unwrap();

        // Then
        assert_eq!(
            entries,
            vec![IndexEntry::Term {
                primary: "Section 3: Advanced Topics".to_string(),
                subentry: None,
                main: false,
            }]
        );
    }

    #[test]
    fn test_parse_index_line_bang_before_type_keyword_marks_main() {
        // Given — real CPython usage: `! pair: statement; if`
        let line = "! pair: statement; if";

        // When
        let entries = parse_index_line(line).unwrap();

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

        // When
        let entries = parse_index_line(line).unwrap();

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

        // When
        let entries = parse_index_line(line).unwrap();

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

        // When
        let entries = parse_index_line(line).unwrap();

        // Then
        assert_eq!(
            entries,
            vec![IndexEntry::Term {
                primary: ", (comma)".to_string(),
                subentry: Some("in string formatting".to_string()),
                main: false,
            }]
        );
    }

    #[test]
    fn test_parse_index_line_bare_shorthand_term_may_contain_semicolon() {
        // Given — a fragment of real CPython usage (Doc/c-api/code.rst:3):
        // an untyped bare term never gets subentry-split on `;`.
        let line = "object; code";

        // When
        let entries = parse_index_line(line).unwrap();

        // Then
        assert_eq!(
            entries,
            vec![IndexEntry::Term {
                primary: "object; code".to_string(),
                subentry: None,
                main: false,
            }]
        );
    }

    #[test]
    fn test_parse_index_line_bare_comma_shorthand_with_semicolon_in_first_term() {
        // Given — real CPython usage (Doc/c-api/code.rst:3): a comma
        // separates two bare terms, the first of which itself contains a
        // literal `;`.
        let line = "object; code, code object";

        // When
        let entries = parse_index_line(line).unwrap();

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
    }

    #[test]
    fn test_parse_index_line_produces_one_term_per_shorthand_value() {
        // Given
        let line = "BNF, grammar, syntax";

        // When
        let entries = parse_index_line(line).unwrap();

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

        // When
        let entries = parse_index_line(line).unwrap();

        // Then
        assert!(entries.is_empty());
    }

    #[test]
    fn test_parse_index_line_parses_single_line_typed_form() {
        // Given — the common single-line `.. index:: single: execution` form,
        // with no indented body block at all
        let line = "single: execution";

        // When
        let entries = parse_index_line(line).unwrap();

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

        // When
        let entries = parse_index_line(line).unwrap();

        // Then
        assert_eq!(
            entries,
            vec![IndexEntry::Term {
                primary: "bogus: foo".to_string(),
                subentry: None,
                main: false,
            }]
        );
    }

    #[test]
    fn test_parse_index_entries_combines_shorthand_and_body_lines() {
        // Given: 2 shorthand entries (BNF, grammar) + 1 single + 1 pair (expands to 2)
        let argument = "BNF, grammar";
        let body_lines = vec!["   single: execution", "   pair: loop; statement"];
        let mut diagnostics = Diagnostics::default();

        // When
        let entries = parse_index_entries(
            argument,
            &body_lines,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

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
        let mut diagnostics = Diagnostics::default();

        // When
        let entries = parse_index_entries(
            argument,
            &body_lines,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

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
        let mut diagnostics = Diagnostics::default();

        // When
        let entries = parse_index_entries(
            argument,
            &body_lines,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

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
        let mut diagnostics = Diagnostics::default();

        // When
        let entries = parse_index_entries(
            argument,
            &body_lines,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

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
        let mut diagnostics = Diagnostics::default();

        // When
        let entries = parse_index_entries(
            argument,
            &body_lines,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

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
        let mut diagnostics = Diagnostics::default();

        // When
        let directive = parse_index_directive(
            argument,
            &body_lines,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        if let rinx_ast::Directive::Index { entries, id } = directive {
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
        let mut diagnostics = Diagnostics::default();

        // When
        let entries = parse_index_entries(
            argument,
            &body_lines,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert!(entries.is_empty());
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_index_line_marks_only_the_bare_terms_written_with_a_bang() {
        // Given — Sphinx reads `!` per comma-separated value
        let line = "! BNF, grammar, !syntax";

        // When
        let entries = parse_index_line(line).unwrap();

        // Then
        let marked: Vec<(&str, bool)> = entries
            .iter()
            .map(|entry| match entry {
                IndexEntry::Term { primary, main, .. } => (primary.as_str(), *main),
                _ => panic!("unexpected {entry:?}"),
            })
            .collect();
        assert_eq!(
            marked,
            vec![("BNF", true), ("grammar", false), ("syntax", true)]
        );
    }

    #[test]
    fn test_parse_index_line_returns_a_malformed_typed_entry_as_an_error() {
        // Given
        let line = "! pair: loop";

        // When
        let result = parse_index_line(line);

        // Then
        assert_eq!(
            result,
            Err(InvalidIndexEntry {
                entry_type: IndexEntryType::Pair,
                value: "loop".to_string(),
            })
        );
    }

    #[test]
    fn test_directive_code_names_the_directive_family_for_every_type() {
        // Given / When / Then
        for entry_type in IndexEntryType::ALL {
            assert!(
                directive_code(entry_type).as_str().starts_with("index."),
                "{entry_type:?}"
            );
        }
    }

    #[test]
    fn test_parse_index_entries_reports_a_malformed_body_line_and_keeps_the_rest() {
        // Given
        let body_lines = vec!["   pair: loop", "   single: execution"];
        let mut diagnostics = Diagnostics::default();

        // When
        let entries = parse_index_entries(
            "",
            &body_lines,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert_eq!(entries.len(), 1);
        let reported: Vec<(DiagnosticCode, String)> = diagnostics
            .into_parts()
            .0
            .into_iter()
            .map(|diagnostic| (diagnostic.code, diagnostic.message))
            .collect();
        assert_eq!(
            reported,
            vec![(
                DiagnosticCode::IndexInvalidPair,
                "invalid pair index entry 'loop'".to_string()
            )]
        );
    }
}
