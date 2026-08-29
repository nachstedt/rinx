//! Enumerator recognition: matching a line's leading enumerator text
//! against every known format/sequence, and deciding whether a
//! candidate really opens a list item. Shared low-level primitives for
//! `super::enumerated_list`.

use crate::blocks::indent_width;
use crate::bullet_list::strip_indent;
use rusty_sphinx_ast::{Enumerator, EnumeratorFormat, EnumeratorSequence};

/// A line's enumerator, as recognised by [`detect_enumerator`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct EnumeratorMatch {
    /// The enumerator this line carries, or `None` when the text is shaped like
    /// one but denotes no ordinal (`mmmmm.`, or an arabic run too large to
    /// hold). docutils treats that as "not a list item".
    pub(super) enumerator: Option<Enumerator>,
    /// Whether the source wrote the `#` auto-enumerator rather than a literal
    /// position.
    pub(super) is_auto: bool,
    /// Columns of whitespace before the enumerator.
    pub(super) item_indent: usize,
    /// The column the item's body starts at — the first non-space after the
    /// marker, or `None` when the marker is alone on its line and the body
    /// indent has to come from the following line.
    pub(super) body_indent: Option<usize>,
}

/// Splits a line into its enumerator text and the rest, for one format.
///
/// Returns the enumerator text and the byte offset just past the punctuation.
/// docutils' patterns require the marker to be followed by one or more spaces
/// or the end of the line, which is what stops `1.5 is a number` from opening a
/// list.
fn split_marker(rest: &str, format: EnumeratorFormat) -> Option<(&str, usize)> {
    let after_prefix = rest.strip_prefix(format.prefix())?;
    let suffix = format.suffix();
    let text_len = after_prefix.find(suffix)?;
    let text = &after_prefix[..text_len];
    let after_marker = &after_prefix[text_len + suffix.len()..];
    if !after_marker.is_empty() && !after_marker.starts_with(' ') {
        return None;
    }
    let consumed = rest.len() - after_marker.len();
    Some((text, consumed))
}

/// Resolves which sequence an enumerator text belongs to.
///
/// This reproduces docutils' `parse_enumerator` chain exactly, including the
/// detail that supplying an `expected` sequence *suppresses* the `i`/`I` roman
/// seeds — in docutils they sit in an `elif` after `elif expected_sequence:`,
/// so they are unreachable once a list is under way. That is why `1.` followed
/// by `i.` reads `i` as the 9th letter (starting a new lower-alpha list at
/// ordinal 9) rather than as roman one.
fn resolve_sequence(
    text: &str,
    expected: Option<EnumeratorSequence>,
) -> Option<EnumeratorSequence> {
    if let Some(expected) = expected {
        if expected.matches(text) {
            return Some(expected);
        }
    } else if text == "i" {
        return Some(EnumeratorSequence::LowerRoman);
    } else if text == "I" {
        return Some(EnumeratorSequence::UpperRoman);
    }
    EnumeratorSequence::RESOLUTION_ORDER
        .into_iter()
        .find(|sequence| sequence.matches(text))
}

/// Recognises an enumerator at the head of `line`.
///
/// `expected` is the sequence the enclosing list has already established, if
/// any; see [`resolve_sequence`] for why passing it changes the outcome.
pub(super) fn detect_enumerator(
    line: &str,
    expected: Option<EnumeratorSequence>,
) -> Option<EnumeratorMatch> {
    let item_indent = indent_width(line);
    let rest = strip_indent(line, item_indent);

    for format in EnumeratorFormat::PROBE_ORDER {
        let Some((text, consumed)) = split_marker(rest, format) else {
            continue;
        };

        let (enumerator, is_auto) = if text == "#" {
            // docutils reports the auto-enumerator as arabic ordinal 1; which
            // position it actually stands for is tracked by the item loop.
            (
                Enumerator::new(EnumeratorSequence::Arabic, format, 1).ok(),
                true,
            )
        } else {
            // No sequence claims this text, so it is not an enumerator in this
            // format — try the next one, as docutils' single alternation regex
            // does by backtracking.
            let Some(sequence) = resolve_sequence(text, expected) else {
                continue;
            };
            let enumerator = sequence
                .ordinal_of(text)
                .and_then(|ordinal| Enumerator::new(sequence, format, ordinal).ok());
            (enumerator, false)
        };

        let after_marker = strip_indent(rest, consumed);
        let body_indent = if after_marker.trim().is_empty() {
            None
        } else {
            Some(item_indent + consumed + indent_width(after_marker))
        };

        return Some(EnumeratorMatch {
            enumerator,
            is_auto,
            item_indent,
            body_indent,
        });
    }
    None
}

/// Whether the line at `i` really opens an enumerated-list item, or is prose
/// that merely starts like one.
///
/// docutils' rule, made indent-relative: the following line must be absent,
/// blank, indented further than the enumerator, or carry the next enumerator in
/// the sequence at the *same* indent. The trailing space after the successor's
/// marker is deliberate — a bare `2.` with nothing after it does not continue a
/// list, matching docutils' `next_enumerator = prefix + text + suffix + ' '`.
pub(super) fn is_enumerated_list_item(
    lines: &[&str],
    i: usize,
    enumerator: Enumerator,
    is_auto: bool,
) -> bool {
    let Some(next_line) = lines.get(i + 1).map(|l| l.trim_end()) else {
        return true;
    };
    if next_line.trim().is_empty() {
        return true;
    }

    let item_indent = indent_width(lines[i]);
    let next_indent = indent_width(next_line);
    if next_indent > item_indent {
        return true;
    }
    if next_indent != item_indent {
        return false;
    }

    let format = enumerator.format();
    let auto_marker = format!("{}#{} ", format.prefix(), format.suffix());
    let next_rest = strip_indent(next_line, next_indent);
    if next_rest.starts_with(&auto_marker) {
        return true;
    }
    // An auto-enumerated item stands for whatever position the list has reached,
    // so only its own `#` form can be predicted, never a literal successor.
    if is_auto {
        return false;
    }
    enumerator
        .successor()
        .is_some_and(|next| next_rest.starts_with(&format!("{next} ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_enumerator_reports_the_body_indent_of_a_wide_marker() {
        // Given markers of different widths
        let narrow = detect_enumerator("9. text", None).expect("should detect");
        let wide = detect_enumerator("10. text", None).expect("should detect");

        // When comparing their body indents
        // Then each accounts for its own marker length
        assert_eq!(narrow.body_indent, Some(3));
        assert_eq!(wide.body_indent, Some(4));
    }

    #[test]
    fn test_detect_enumerator_reports_no_body_indent_for_a_bare_marker() {
        // Given a marker alone on its line
        let detected = detect_enumerator("1.", None).expect("should detect");

        // When reading the body indent
        // Then none is known from this line alone
        assert_eq!(detected.body_indent, None);
    }

    #[test]
    fn test_detect_enumerator_reports_the_item_indent() {
        // Given an indented marker
        let detected = detect_enumerator("   1. text", None).expect("should detect");

        // When reading the item indent
        // Then it counts the leading whitespace
        assert_eq!(detected.item_indent, 3);
        assert_eq!(detected.body_indent, Some(6));
    }

    #[test]
    fn test_detect_enumerator_skips_extra_spaces_after_the_marker() {
        // Given a marker followed by several spaces
        let detected = detect_enumerator("1.   text", None).expect("should detect");

        // When reading the body indent
        // Then it points at the text, not at the first space
        assert_eq!(detected.body_indent, Some(5));
    }

    #[test]
    fn test_detect_enumerator_flags_the_auto_enumerator() {
        // Given the `#` marker
        let detected = detect_enumerator("#. text", None).expect("should detect");

        // When inspecting the match
        // Then it is flagged as auto and reported as arabic one
        assert!(detected.is_auto);
        assert_eq!(
            detected.enumerator.map(|e| e.sequence()),
            Some(EnumeratorSequence::Arabic)
        );
    }

    #[test]
    fn test_detect_enumerator_declines_lines_without_a_marker() {
        // Given lines that carry no enumerator
        let inputs = ["plain text", "* bullet", "1.5 no", "(1)no space", ". empty"];

        // When detecting an enumerator on each
        // Then none is found
        for input in inputs {
            assert!(detect_enumerator(input, None).is_none(), "{input:?}");
        }
    }

    #[test]
    fn test_detect_enumerator_accepts_a_marker_at_end_of_line() {
        // Given each format with nothing after the marker
        for format in EnumeratorFormat::PROBE_ORDER {
            let line = format!("{}1{}", format.prefix(), format.suffix());

            // When detecting an enumerator
            let detected = detect_enumerator(&line, None);

            // Then it is recognised, since docutils allows the marker to end
            // the line
            assert!(detected.is_some(), "{line:?}");
        }
    }

    #[test]
    fn test_split_marker_requires_a_space_after_the_punctuation() {
        // Given text where the marker is run together with what follows
        // When splitting it
        // Then no marker is found
        assert_eq!(split_marker("1.text", EnumeratorFormat::Period), None);
        assert_eq!(split_marker("1)text", EnumeratorFormat::RightParen), None);
    }

    #[test]
    fn test_split_marker_reports_the_text_and_consumed_width() {
        // Given a parenthesised marker
        // When splitting it
        let split = split_marker("(iv) text", EnumeratorFormat::Parens);

        // Then the enumerator text excludes the punctuation, and the width
        // covers all of it
        assert_eq!(split, Some(("iv", 4)));
    }

    #[test]
    fn test_resolve_sequence_prefers_the_expected_sequence() {
        // Given text valid in more than one sequence, and an expectation
        // When resolving it
        // Then the expected sequence wins
        assert_eq!(
            resolve_sequence("i", Some(EnumeratorSequence::UpperAlpha)),
            Some(EnumeratorSequence::LowerAlpha)
        );
        assert_eq!(
            resolve_sequence("i", Some(EnumeratorSequence::LowerAlpha)),
            Some(EnumeratorSequence::LowerAlpha)
        );
        assert_eq!(
            resolve_sequence("i", Some(EnumeratorSequence::LowerRoman)),
            Some(EnumeratorSequence::LowerRoman)
        );
    }

    #[test]
    fn test_resolve_sequence_seeds_roman_only_without_an_expectation() {
        // Given the roman seed texts
        // When resolving them with no expectation
        // Then roman is chosen
        assert_eq!(
            resolve_sequence("i", None),
            Some(EnumeratorSequence::LowerRoman)
        );
        assert_eq!(
            resolve_sequence("I", None),
            Some(EnumeratorSequence::UpperRoman)
        );
    }

    #[test]
    fn test_resolve_sequence_falls_back_to_the_generic_order() {
        // Given text no expectation can claim
        // When resolving it
        // Then the first sequence in resolution order that matches is used,
        // and the roman seeds stay unreachable
        assert_eq!(
            resolve_sequence("i", Some(EnumeratorSequence::Arabic)),
            Some(EnumeratorSequence::LowerAlpha)
        );
        assert_eq!(
            resolve_sequence("42", None),
            Some(EnumeratorSequence::Arabic)
        );
        assert_eq!(resolve_sequence("?", None), None);
    }

    #[test]
    fn test_is_enumerated_list_item_accepts_a_blank_or_absent_next_line() {
        // Given an enumerator whose next line is blank, or which ends the input
        let enumerator =
            Enumerator::new(EnumeratorSequence::Arabic, EnumeratorFormat::Period, 1).unwrap();

        // When testing it
        // Then both shapes are accepted
        assert!(is_enumerated_list_item(&["1. a", ""], 0, enumerator, false));
        assert!(is_enumerated_list_item(&["1. a"], 0, enumerator, false));
    }

    #[test]
    fn test_is_enumerated_list_item_accepts_an_indented_next_line() {
        // Given an enumerator whose next line is indented further
        let enumerator =
            Enumerator::new(EnumeratorSequence::Arabic, EnumeratorFormat::Period, 1).unwrap();

        // When testing it
        // Then it is accepted as a continuation
        assert!(is_enumerated_list_item(
            &["1. a", "   more"],
            0,
            enumerator,
            false
        ));
    }

    #[test]
    fn test_is_enumerated_list_item_rejects_a_sibling_line_at_the_list_indent() {
        // Given an indented list whose next line sits at the same indent but is
        // not the successor
        let enumerator =
            Enumerator::new(EnumeratorSequence::Arabic, EnumeratorFormat::Period, 1).unwrap();

        // When testing it
        // Then it is rejected — the indent comparison has to be relative to the
        // enumerator, not to column zero
        assert!(!is_enumerated_list_item(
            &["   1. a", "   Not a continuation"],
            0,
            enumerator,
            false
        ));
    }

    #[test]
    fn test_is_enumerated_list_item_accepts_the_auto_marker_as_a_successor() {
        // Given an explicit enumerator followed by an auto-enumerated item
        let enumerator =
            Enumerator::new(EnumeratorSequence::Arabic, EnumeratorFormat::Period, 1).unwrap();

        // When testing it
        // Then the `#` marker counts as the next enumerator
        assert!(is_enumerated_list_item(
            &["1. a", "#. b"],
            0,
            enumerator,
            false
        ));
    }

    #[test]
    fn test_is_enumerated_list_item_rejects_a_literal_successor_after_an_auto_item() {
        // Given an auto-enumerated item followed by an explicit enumerator
        let enumerator =
            Enumerator::new(EnumeratorSequence::Arabic, EnumeratorFormat::Period, 1).unwrap();

        // When testing it
        // Then it is rejected: an auto item stands for an unknown position, so
        // only another `#` can be predicted to follow it
        assert!(!is_enumerated_list_item(
            &["#. a", "2. b"],
            0,
            enumerator,
            true
        ));
    }

    #[test]
    fn test_is_enumerated_list_item_rejects_a_successor_at_the_end_of_a_sequence() {
        // Given the last enumerator its sequence can express
        let enumerator =
            Enumerator::new(EnumeratorSequence::LowerAlpha, EnumeratorFormat::Period, 26).unwrap();

        // When testing it against a following unindented line
        // Then it is rejected, since no successor exists to match against
        assert!(!is_enumerated_list_item(
            &["z. a", "next line"],
            0,
            enumerator,
            false
        ));
    }
}
