//! Choosing which part of a file a directive splices in.
//!
//! `.. include::` and `.. literalinclude::` both read a whole file and then
//! keep a part of it, and they spell that choice in overlapping vocabularies:
//! docutils' `:start-line:`/`:end-line:`/`:start-after:`/`:end-before:` on the
//! first, Sphinx's `:lines:`/`:start-at:`/`:end-at:` added on the second. The
//! two are one operation over a list of lines, so they are implemented once
//! here rather than twice.
//!
//! Every way of selecting nothing is diagnosed rather than yielding an empty
//! block. A `:start-after:` whose text was never found is a typo in the
//! document nine times out of ten, and a directive that answered it with
//! silence would leave the author looking at a page with a section quietly
//! missing.
//!
//! Line numbers in this module are **1-based**, the way an author counts them
//! and the way [`SelectedText::first_line`] must report them for
//! `:lineno-match:`. The `:start-line:`/`:end-line:` options are the exception
//! docutils made — they are 0-based indices — and that conversion happens at
//! the edge, in [`Selection::apply`], so nothing downstream has to remember it.

use std::num::NonZeroU32;

use rusty_sphinx_ast::{Diagnostic, DiagnosticCode, Span};

use crate::diagnostics::Diagnostics;

/// The text a directive selected out of a file, and where it started.
pub(in crate::directives) struct SelectedText {
    /// The selected lines, joined with `\n` and with no trailing newline.
    pub text: String,
    /// The 1-based number, *in the file*, of the first selected line, or
    /// `None` when the selection is not one contiguous run.
    ///
    /// `:lineno-match:` is the only consumer, and a discontinuous selection is
    /// exactly the case it cannot serve — see
    /// [`DiagnosticCode::LiteralIncludeLinenoMatchUnusable`].
    pub first_line: Option<NonZeroU32>,
}

/// What a directive asked to take out of the file it named.
///
/// Every field is the raw option as written; nothing is validated until
/// [`Self::apply`], which is the only place the file's actual contents are
/// known and so the only place "line 40 of a 12-line file" can be detected.
#[derive(Default)]
pub(in crate::directives) struct Selection {
    /// `:lines:` — a comma-separated list of numbers and `3-5` ranges,
    /// 1-based, as written.
    pub lines: Option<String>,
    /// `:start-line:` — a **0-based** index; the line at it is included.
    pub start_line: Option<usize>,
    /// `:end-line:` — a **0-based** index; the line at it is *excluded*.
    pub end_line: Option<usize>,
    /// `:start-after:` — begin *after* the line containing this text.
    pub start_after: Option<String>,
    /// `:end-before:` — end *before* the line containing this text.
    pub end_before: Option<String>,
    /// `:start-at:` — begin *at* the line containing this text.
    pub start_at: Option<String>,
    /// `:end-at:` — end *after* the line containing this text.
    pub end_at: Option<String>,
}

impl Selection {
    /// Whether any selection option was written at all.
    pub(in crate::directives) const fn is_empty(&self) -> bool {
        self.lines.is_none()
            && self.start_line.is_none()
            && self.end_line.is_none()
            && self.start_after.is_none()
            && self.end_before.is_none()
            && self.start_at.is_none()
            && self.end_at.is_none()
    }

    /// Applies this selection to `text`, reporting whatever made it fail.
    ///
    /// Returns `None` when nothing could be selected, having already pushed a
    /// diagnostic; the caller degrades the directive rather than emitting an
    /// empty block. `span` is the directive's own, since the file's lines have
    /// no position in the document being parsed.
    ///
    /// Order matches Sphinx's `LiteralIncludeReader`: the line-addressed
    /// options narrow first, then the text-addressed ones search what is left.
    /// That order is observable — a `:start-after:` after a `:lines: 10-20`
    /// searches only those eleven lines — so it is part of the contract.
    pub(in crate::directives) fn apply(
        &self,
        text: &str,
        directive: &str,
        diagnostics: &mut Diagnostics,
        span: Option<Span>,
    ) -> Option<SelectedText> {
        let all: Vec<&str> = text.lines().collect();
        let total = all.len();

        let (kept, first_line, contiguous) =
            self.select_lines(&all, directive, diagnostics, span)?;
        let (kept, first_line) =
            self.apply_text_bounds(&kept, first_line, directive, diagnostics, span)?;

        if kept.is_empty() {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::IncludeEmptySelection,
                format!(
                    "{directive}: the selection options match no lines of the file, \
                     which has {total}"
                ),
                span,
            ));
            return None;
        }

        Some(SelectedText {
            text: kept.join("\n"),
            first_line: if contiguous {
                NonZeroU32::new(u32::try_from(first_line).unwrap_or(u32::MAX))
            } else {
                None
            },
        })
    }

    /// Applies `:lines:` or `:start-line:`/`:end-line:`, whichever was
    /// written, yielding the surviving lines, the 1-based number of the first
    /// of them, and whether they are one contiguous run.
    fn select_lines<'a>(
        &self,
        all: &[&'a str],
        directive: &str,
        diagnostics: &mut Diagnostics,
        span: Option<Span>,
    ) -> Option<(Vec<&'a str>, usize, bool)> {
        let total = all.len();
        if let Some(written) = &self.lines {
            let numbers = parse_line_list(written, directive, diagnostics, span)?;
            let mut kept = Vec::new();
            for number in &numbers {
                let Some(line) = all.get(number - 1) else {
                    diagnostics.push(Diagnostic::at(
                        DiagnosticCode::IncludeInvalidLineRange,
                        format!(
                            "{directive}: :lines: names line {number}, but the file has {total}"
                        ),
                        span,
                    ));
                    return None;
                };
                kept.push(*line);
            }
            let first = numbers.first().copied().unwrap_or(1);
            let contiguous = numbers.windows(2).all(|pair| pair[1] == pair[0] + 1);
            return Some((kept, first, contiguous));
        }

        // docutils' 0-based, end-exclusive pair.
        let start = self.start_line.unwrap_or(0);
        let end = self.end_line.unwrap_or(total);
        if start > total {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::IncludeInvalidLineRange,
                format!(
                    "{directive}: :start-line: {start} is past the end of the file, \
                     which has {total} line(s)"
                ),
                span,
            ));
            return None;
        }
        if end < start {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::IncludeInvalidLineRange,
                format!("{directive}: :end-line: {end} is before :start-line: {start}"),
                span,
            ));
            return None;
        }
        let end = end.min(total);
        Some((all[start..end].to_vec(), start + 1, true))
    }

    /// Applies the text-addressed bounds to an already-narrowed run of lines.
    fn apply_text_bounds<'a>(
        &self,
        lines: &[&'a str],
        first_line: usize,
        directive: &str,
        diagnostics: &mut Diagnostics,
        span: Option<Span>,
    ) -> Option<(Vec<&'a str>, usize)> {
        let mut start = 0;
        let mut end = lines.len();

        if let Some(needle) = &self.start_after {
            // *After* the matching line, so the marker itself stays out.
            start = find_line(lines, needle, directive, ":start-after:", diagnostics, span)? + 1;
        } else if let Some(needle) = &self.start_at {
            start = find_line(lines, needle, directive, ":start-at:", diagnostics, span)?;
        }

        if let Some(needle) = &self.end_before {
            end = find_line(lines, needle, directive, ":end-before:", diagnostics, span)?;
        } else if let Some(needle) = &self.end_at {
            end = find_line(lines, needle, directive, ":end-at:", diagnostics, span)? + 1;
        }

        // A start past the end is not an error in itself — it is the empty
        // selection the caller diagnoses, with a message that names the file's
        // real size rather than an index the author never wrote.
        let (start, end) = (start.min(lines.len()), end.min(lines.len()));
        if start >= end {
            return Some((Vec::new(), first_line + start));
        }
        Some((lines[start..end].to_vec(), first_line + start))
    }
}

/// The index of the first line containing `needle`, reported if there is none.
fn find_line(
    lines: &[&str],
    needle: &str,
    directive: &str,
    option: &str,
    diagnostics: &mut Diagnostics,
    span: Option<Span>,
) -> Option<usize> {
    let found = lines.iter().position(|line| line.contains(needle));
    if found.is_none() {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::IncludeTextNotFound,
            format!("{directive}: {option} text '{needle}' appears nowhere in the file"),
            span,
        ));
    }
    found
}

/// Expands a `:lines:` value into 1-based line numbers, in the order written.
///
/// Order is kept rather than sorted, and duplicates are kept too, because
/// `:lines: 5,1` is a request to show line 5 then line 1 — unlike
/// `:emphasize-lines:`, which decorates lines the block already has and so
/// normalizes.
fn parse_line_list(
    written: &str,
    directive: &str,
    diagnostics: &mut Diagnostics,
    span: Option<Span>,
) -> Option<Vec<usize>> {
    let mut numbers = Vec::new();
    for part in written.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let expanded = match part.split_once('-') {
            Some((first, last)) => {
                let first = parse_line_number(first, part, directive, diagnostics, span)?;
                let last = parse_line_number(last, part, directive, diagnostics, span)?;
                if last < first {
                    diagnostics.push(Diagnostic::at(
                        DiagnosticCode::IncludeInvalidLineRange,
                        format!("{directive}: :lines: range '{part}' ends before it begins"),
                        span,
                    ));
                    return None;
                }
                (first..=last).collect()
            }
            None => vec![parse_line_number(part, part, directive, diagnostics, span)?],
        };
        numbers.extend(expanded);
    }
    if numbers.is_empty() {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::IncludeInvalidLineRange,
            format!("{directive}: :lines: names no lines"),
            span,
        ));
        return None;
    }
    Some(numbers)
}

/// One line number from a `:lines:` value: a positive integer, since line zero
/// is not a thing an author can mean.
fn parse_line_number(
    text: &str,
    part: &str,
    directive: &str,
    diagnostics: &mut Diagnostics,
    span: Option<Span>,
) -> Option<usize> {
    let Ok(number) = text.trim().parse::<NonZeroU32>() else {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::IncludeInvalidLineRange,
            format!("{directive}: :lines: entry '{part}' is not a positive line number"),
            span,
        ));
        return None;
    };
    Some(number.get() as usize)
}

/// Accepts only the encodings that are UTF-8 or a subset of it.
///
/// The same narrowing `.. csv-table::`'s `:encoding:` already makes, and for
/// the same reason: this build reads files as UTF-8, so claiming to honour
/// `latin-1` would mean silently mis-decoding it. Reported rather than
/// ignored, so an author who needs another encoding learns it here.
pub(in crate::directives) fn check_encoding(
    encoding: Option<&str>,
    directive: &str,
    diagnostics: &mut Diagnostics,
    span: Option<Span>,
) -> bool {
    let Some(encoding) = encoding else {
        return true;
    };
    let normalized = encoding.trim().to_ascii_lowercase().replace('_', "-");
    if matches!(normalized.as_str(), "utf-8" | "utf8" | "ascii" | "us-ascii") {
        return true;
    }
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::IncludeEncodingUnsupported,
        format!("{directive}: :encoding: '{encoding}' is unsupported; files are read as UTF-8"),
        span,
    ));
    false
}

/// Replaces each tab with enough spaces to reach the next multiple of `width`.
///
/// docutils' `:tab-width:`. A negative width means "leave tabs alone", which is
/// how docutils spells the opt-out, so it is a signed value rather than a
/// `usize` that would have to invent a sentinel.
pub(in crate::directives) fn expand_tabs(text: &str, width: i32) -> String {
    // A non-positive width is docutils' "leave tabs alone", so the conversion
    // below cannot fail — but it is written fallibly rather than cast, since a
    // silent wrap here would pad every line to four billion columns.
    let Ok(width) = usize::try_from(width) else {
        return text.to_string();
    };
    if width == 0 {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let mut column = 0;
        for character in line.chars() {
            if character == '\t' {
                let spaces = width - (column % width);
                out.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            } else {
                out.push(character);
                column += 1;
            }
        }
        out.push('\n');
    }
    // `lines()` dropped the final newline; restore only what was there.
    if !text.ends_with('\n') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::{Position, Span};

    const FILE: &str = "one\ntwo\nthree\nfour\nfive";

    fn a_span() -> Span {
        Span::new(Position::new(1, 1), Position::new(1, 20))
    }

    /// Applies `selection` to [`FILE`], returning the text and the collector.
    fn select(selection: &Selection) -> (Option<SelectedText>, Diagnostics) {
        let mut diagnostics = Diagnostics::default();
        let selected = selection.apply(FILE, "literalinclude", &mut diagnostics, Some(a_span()));
        (selected, diagnostics)
    }

    fn codes(diagnostics: &Diagnostics) -> Vec<DiagnosticCode> {
        diagnostics.iter().map(|d| d.code).collect()
    }

    #[test]
    fn test_an_empty_selection_is_recognized_as_such() {
        // Given / When
        let selection = Selection::default();

        // Then
        assert!(selection.is_empty());
    }

    #[test]
    fn test_a_selection_with_any_option_is_not_empty() {
        // Given / When
        let selection = Selection {
            lines: Some("1".to_string()),
            ..Selection::default()
        };

        // Then
        assert!(!selection.is_empty());
    }

    #[test]
    fn test_no_options_selects_the_whole_file() {
        // Given / When
        let (selected, diagnostics) = select(&Selection::default());

        // Then
        let selected = selected.expect("the whole file is a valid selection");
        assert_eq!(selected.text, FILE);
        assert_eq!(selected.first_line, NonZeroU32::new(1));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_lines_selects_the_named_lines() {
        // Given
        let selection = Selection {
            lines: Some("2-3".to_string()),
            ..Selection::default()
        };

        // When
        let (selected, _) = select(&selection);

        // Then
        let selected = selected.expect("a valid range");
        assert_eq!(selected.text, "two\nthree");
        assert_eq!(selected.first_line, NonZeroU32::new(2));
    }

    #[test]
    fn test_lines_accepts_a_mixed_list() {
        // Given
        let selection = Selection {
            lines: Some("1,3-4".to_string()),
            ..Selection::default()
        };

        // When
        let (selected, _) = select(&selection);

        // Then
        assert_eq!(selected.expect("a valid list").text, "one\nthree\nfour");
    }

    #[test]
    fn test_a_discontinuous_lines_selection_has_no_first_line() {
        // Given a list with a gap
        let selection = Selection {
            lines: Some("1,3".to_string()),
            ..Selection::default()
        };

        // When
        let (selected, _) = select(&selection);

        // Then — `:lineno-match:` cannot number this, and says so rather than
        // picking one of the two runs.
        assert_eq!(selected.expect("a valid list").first_line, None);
    }

    #[test]
    fn test_lines_past_the_end_of_the_file_is_reported() {
        // Given
        let selection = Selection {
            lines: Some("9".to_string()),
            ..Selection::default()
        };

        // When
        let (selected, diagnostics) = select(&selection);

        // Then
        assert!(selected.is_none());
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::IncludeInvalidLineRange]
        );
        assert!(
            diagnostics[0].message.contains("the file has 5"),
            "{}",
            diagnostics[0].message
        );
    }

    #[test]
    fn test_a_backwards_lines_range_is_reported() {
        // Given
        let selection = Selection {
            lines: Some("4-2".to_string()),
            ..Selection::default()
        };

        // When
        let (selected, diagnostics) = select(&selection);

        // Then
        assert!(selected.is_none());
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::IncludeInvalidLineRange]
        );
    }

    #[test]
    fn test_a_non_numeric_lines_entry_is_reported() {
        // Given
        let selection = Selection {
            lines: Some("two".to_string()),
            ..Selection::default()
        };

        // When
        let (selected, diagnostics) = select(&selection);

        // Then
        assert!(selected.is_none());
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::IncludeInvalidLineRange]
        );
    }

    #[test]
    fn test_line_zero_is_reported_rather_than_accepted() {
        // Given — an author counting from zero, which reST never does
        let selection = Selection {
            lines: Some("0".to_string()),
            ..Selection::default()
        };

        // When
        let (selected, diagnostics) = select(&selection);

        // Then
        assert!(selected.is_none());
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::IncludeInvalidLineRange]
        );
    }

    #[test]
    fn test_start_line_and_end_line_are_zero_based_and_end_exclusive() {
        // Given docutils' pair, which indexes from zero unlike everything else
        let selection = Selection {
            start_line: Some(1),
            end_line: Some(3),
            ..Selection::default()
        };

        // When
        let (selected, _) = select(&selection);

        // Then — indices 1 and 2, which an author reads as lines 2 and 3
        let selected = selected.expect("a valid range");
        assert_eq!(selected.text, "two\nthree");
        assert_eq!(selected.first_line, NonZeroU32::new(2));
    }

    #[test]
    fn test_an_end_line_past_the_file_is_clamped_rather_than_reported() {
        // Given — docutils treats this as "to the end", not as a mistake
        let selection = Selection {
            start_line: Some(3),
            end_line: Some(99),
            ..Selection::default()
        };

        // When
        let (selected, diagnostics) = select(&selection);

        // Then
        assert_eq!(selected.expect("a valid range").text, "four\nfive");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_a_start_line_past_the_file_is_reported() {
        // Given
        let selection = Selection {
            start_line: Some(99),
            ..Selection::default()
        };

        // When
        let (selected, diagnostics) = select(&selection);

        // Then
        assert!(selected.is_none());
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::IncludeInvalidLineRange]
        );
    }

    #[test]
    fn test_an_end_line_before_the_start_line_is_reported() {
        // Given
        let selection = Selection {
            start_line: Some(3),
            end_line: Some(1),
            ..Selection::default()
        };

        // When
        let (selected, diagnostics) = select(&selection);

        // Then
        assert!(selected.is_none());
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::IncludeInvalidLineRange]
        );
    }

    #[test]
    fn test_start_after_begins_below_the_matching_line() {
        // Given
        let selection = Selection {
            start_after: Some("two".to_string()),
            ..Selection::default()
        };

        // When
        let (selected, _) = select(&selection);

        // Then — the marker line itself stays out
        let selected = selected.expect("a valid selection");
        assert_eq!(selected.text, "three\nfour\nfive");
        assert_eq!(selected.first_line, NonZeroU32::new(3));
    }

    #[test]
    fn test_start_at_begins_on_the_matching_line() {
        // Given
        let selection = Selection {
            start_at: Some("two".to_string()),
            ..Selection::default()
        };

        // When
        let (selected, _) = select(&selection);

        // Then — unlike `:start-after:`, the marker is kept
        assert_eq!(
            selected.expect("a valid selection").text,
            "two\nthree\nfour\nfive"
        );
    }

    #[test]
    fn test_end_before_stops_above_the_matching_line() {
        // Given
        let selection = Selection {
            end_before: Some("four".to_string()),
            ..Selection::default()
        };

        // When
        let (selected, _) = select(&selection);

        // Then
        assert_eq!(selected.expect("a valid selection").text, "one\ntwo\nthree");
    }

    #[test]
    fn test_end_at_stops_below_the_matching_line() {
        // Given
        let selection = Selection {
            end_at: Some("four".to_string()),
            ..Selection::default()
        };

        // When
        let (selected, _) = select(&selection);

        // Then
        assert_eq!(
            selected.expect("a valid selection").text,
            "one\ntwo\nthree\nfour"
        );
    }

    #[test]
    fn test_start_after_and_end_before_bracket_a_region() {
        // Given the shape a marked-up example file is written for
        let selection = Selection {
            start_after: Some("one".to_string()),
            end_before: Some("five".to_string()),
            ..Selection::default()
        };

        // When
        let (selected, _) = select(&selection);

        // Then
        assert_eq!(selected.expect("a valid region").text, "two\nthree\nfour");
    }

    #[test]
    fn test_unmatched_start_after_text_is_reported() {
        // Given a marker that is not in the file — usually a typo
        let selection = Selection {
            start_after: Some("nowhere".to_string()),
            ..Selection::default()
        };

        // When
        let (selected, diagnostics) = select(&selection);

        // Then — reported, not silently answered with nothing
        assert!(selected.is_none());
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::IncludeTextNotFound]
        );
        assert!(
            diagnostics[0].message.contains("nowhere"),
            "{}",
            diagnostics[0].message
        );
        assert!(
            diagnostics[0].message.contains(":start-after:"),
            "{}",
            diagnostics[0].message
        );
    }

    #[test]
    fn test_unmatched_end_before_text_is_reported() {
        // Given
        let selection = Selection {
            end_before: Some("nowhere".to_string()),
            ..Selection::default()
        };

        // When
        let (selected, diagnostics) = select(&selection);

        // Then
        assert!(selected.is_none());
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::IncludeTextNotFound]
        );
    }

    #[test]
    fn test_text_bounds_search_only_what_the_line_options_kept() {
        // Given a marker that exists in the file but not in the kept range
        let selection = Selection {
            lines: Some("1-2".to_string()),
            start_after: Some("four".to_string()),
            ..Selection::default()
        };

        // When
        let (selected, diagnostics) = select(&selection);

        // Then — the narrowing happens first, so this is honestly not found
        assert!(selected.is_none());
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::IncludeTextNotFound]
        );
    }

    #[test]
    fn test_bounds_that_cross_select_nothing_and_are_reported() {
        // Given an end above the start
        let selection = Selection {
            start_after: Some("four".to_string()),
            end_before: Some("two".to_string()),
            ..Selection::default()
        };

        // When
        let (selected, diagnostics) = select(&selection);

        // Then
        assert!(selected.is_none());
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::IncludeEmptySelection]
        );
    }

    #[test]
    fn test_start_after_on_the_last_line_selects_nothing_and_is_reported() {
        // Given a marker with no lines below it
        let selection = Selection {
            start_after: Some("five".to_string()),
            ..Selection::default()
        };

        // When
        let (selected, diagnostics) = select(&selection);

        // Then
        assert!(selected.is_none());
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::IncludeEmptySelection]
        );
        assert!(
            diagnostics[0].message.contains("which has 5"),
            "{}",
            diagnostics[0].message
        );
    }

    #[test]
    fn test_check_encoding_accepts_utf8_and_its_subsets() {
        // Given / When / Then
        for encoding in ["utf-8", "UTF-8", "utf8", "ascii", "us-ascii"] {
            let mut diagnostics = Diagnostics::default();
            assert!(
                check_encoding(Some(encoding), "include", &mut diagnostics, Some(a_span())),
                "{encoding} should be accepted"
            );
            assert!(diagnostics.is_empty());
        }
    }

    #[test]
    fn test_check_encoding_accepts_an_absent_option() {
        // Given / When
        let mut diagnostics = Diagnostics::default();
        let accepted = check_encoding(None, "include", &mut diagnostics, Some(a_span()));

        // Then
        assert!(accepted);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_check_encoding_reports_anything_else() {
        // Given
        let mut diagnostics = Diagnostics::default();

        // When
        let accepted = check_encoding(Some("latin-1"), "include", &mut diagnostics, Some(a_span()));

        // Then
        assert!(!accepted);
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::IncludeEncodingUnsupported]
        );
    }

    #[test]
    fn test_expand_tabs_pads_to_the_next_tab_stop() {
        // Given / When — not a fixed number of spaces, but to the next stop
        let expanded = expand_tabs("a\tb", 4);

        // Then
        assert_eq!(expanded, "a   b");
    }

    #[test]
    fn test_expand_tabs_restarts_the_column_count_each_line() {
        // Given / When
        let expanded = expand_tabs("ab\tc\nabc\td", 4);

        // Then
        assert_eq!(expanded, "ab  c\nabc d");
    }

    #[test]
    fn test_expand_tabs_of_a_negative_width_leaves_tabs_alone() {
        // Given — docutils' opt-out spelling
        let expanded = expand_tabs("a\tb", -1);

        // Then
        assert_eq!(expanded, "a\tb");
    }

    #[test]
    fn test_expand_tabs_keeps_a_trailing_newline() {
        // Given / When
        let expanded = expand_tabs("a\tb\n", 4);

        // Then
        assert_eq!(expanded, "a   b\n");
    }

    #[test]
    fn test_expand_tabs_does_not_add_a_trailing_newline() {
        // Given / When
        let expanded = expand_tabs("a\tb", 4);

        // Then
        assert!(!expanded.ends_with('\n'));
    }
}
