//! The enumerated-list item loop: recognizing each item, collecting its
//! body, and deciding when the list ends.

use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;
use crate::indent::indent_width;
use crate::indent::strip_indent;
use rinx_ast::{
    Diagnostic, DiagnosticCode, Enumerator, EnumeratorFormat, EnumeratorSequence, ListItem, Node,
};

use super::diagnostics::diagnose_unrecognised_list;
use super::format::{EnumeratorMatch, detect_enumerator, is_enumerated_list_item};

/// State the item loop carries between items of one enumerated list.
struct ListState {
    start: Enumerator,
    list_indent: usize,
    format: EnumeratorFormat,
    sequence: EnumeratorSequence,
    last_ordinal: u32,
    /// Set once a `#` item has been seen. docutils refuses to mix an explicit
    /// enumerator back in after that, so this ends the list.
    auto_engaged: bool,
}

impl ListState {
    /// Whether `candidate` continues this list, or breaks it so a new one
    /// starts. Mirrors docutils' `EnumeratedList.enumerator` guard.
    fn accepts(&self, candidate: &EnumeratorMatch, enumerator: Enumerator) -> bool {
        if candidate.item_indent != self.list_indent || enumerator.format() != self.format {
            return false;
        }
        if candidate.is_auto {
            return true;
        }
        !self.auto_engaged
            && enumerator.sequence() == self.sequence
            && enumerator.ordinal() == self.last_ordinal + 1
    }
}

/// Parses an enumerated list starting at `lines[start_i]`.
///
/// Returns the number of lines consumed and the list node, or `None` when the
/// line does not open a list.
pub(crate) fn try_parse_enumerated_list(
    lines: &[&str],
    start_i: usize,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<(usize, Node)> {
    let mut items: Vec<ListItem> = Vec::new();
    let mut state: Option<ListState> = None;
    let mut i = start_i;
    // docutils' `blank_finish`: whether the *last item consumed* ended at a
    // blank line or at end of input. A list interrupted by an unindented line
    // instead is what earns the unindent warning.
    let mut blank_finish = true;

    while i < lines.len() {
        let line = lines[i].trim_end();
        if line.trim().is_empty() {
            break;
        }

        let expected = state.as_ref().map(|s| s.sequence);
        let Some(candidate) = detect_enumerator(line, expected) else {
            break;
        };
        let Some(enumerator) = candidate.enumerator else {
            break;
        };
        if let Some(state) = state.as_ref()
            && !state.accepts(&candidate, enumerator)
        {
            break;
        }
        // Checked before the item is consumed, so a line that turns out to be
        // prose is left for `parse_blocks` to re-dispatch as a paragraph.
        if !is_enumerated_list_item(lines, i, &candidate, enumerator) {
            if state.is_none() {
                diagnose_unrecognised_list(lines, i, &candidate, enumerator, diagnostics, ctx);
            }
            break;
        }

        let body_indent = candidate
            .body_indent
            .unwrap_or_else(|| following_body_indent(lines, i, candidate.item_indent));

        // Where this item's body begins, recorded before `i` advances: the
        // body is dedented by `body_indent`, so mapping a position inside it
        // back to the document needs both offsets.
        let item_start = i;
        let (consumed, body_lines, item_blank_finish) =
            collect_item_body(lines, i, candidate, body_indent);
        i += consumed;
        blank_finish = item_blank_finish;

        let body_refs: Vec<&str> = body_lines.iter().map(String::as_str).collect();
        let item_ctx = ctx.nested(item_start, body_indent);
        let nodes = parse_blocks(&body_refs, adornment_order, diagnostics, &item_ctx);
        items.push(ListItem { nodes });

        state = Some(match state {
            None => ListState {
                start: enumerator,
                list_indent: candidate.item_indent,
                format: enumerator.format(),
                sequence: enumerator.sequence(),
                last_ordinal: enumerator.ordinal(),
                auto_engaged: candidate.is_auto,
            },
            Some(mut state) => {
                state.last_ordinal += 1;
                state.auto_engaged |= candidate.is_auto;
                state
            }
        });
    }

    let state = state?;

    if state.start.ordinal() != 1 {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::ListEnumeratedStartNotOne,
            format!(
                "Enumerated list start value not ordinal-1: \"{}\" (ordinal {})",
                state.start.marker_text(),
                state.start.ordinal()
            ),
            ctx.line_span(start_i, lines[start_i]),
        ));
    }
    if !blank_finish {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::ListEnumeratedNoBlankLine,
            "Enumerated list ends without a blank line; unexpected unindent.",
            // The last line the list consumed: the unindent is what follows it.
            ctx.line_span(i - 1, lines[i - 1]),
        ));
    }

    Some((
        i - start_i,
        Node::EnumeratedList {
            start: state.start,
            items,
        },
    ))
}

/// The body indent for an item whose marker is alone on its line.
///
/// docutils reads it from the first following indented line rather than
/// assuming a fixed offset, so `1.` followed by a body indented four columns
/// works. Falls back to one column past the marker when nothing follows.
fn following_body_indent(lines: &[&str], i: usize, item_indent: usize) -> usize {
    lines
        .iter()
        .skip(i + 1)
        .find(|l| !l.trim().is_empty())
        .map(|l| indent_width(l))
        .filter(|indent| *indent > item_indent)
        .unwrap_or(item_indent + 1)
}

/// Collects one item's body lines, dedented to `body_indent`.
///
/// Blank lines are kept so a multi-paragraph item splits correctly, then
/// trimmed off the end — the same shape as [`crate::blocks::bullet_list`]'s loop.
///
/// The third return value is docutils' `blank_finish`: whether the body was
/// terminated by a blank line or by the end of input, rather than by a line
/// dedented back out of the item.
fn collect_item_body(
    lines: &[&str],
    start_i: usize,
    candidate: EnumeratorMatch,
    body_indent: usize,
) -> (usize, Vec<String>, bool) {
    let first_line = lines[start_i].trim_end();
    let mut body_lines = vec![if candidate.body_indent.is_some() {
        strip_indent(first_line, body_indent).to_string()
    } else {
        String::new()
    }];

    let mut i = start_i + 1;
    let mut blank_finish = false;
    while i < lines.len() {
        let next_line = lines[i].trim_end();
        if next_line.trim().is_empty() {
            body_lines.push(String::new());
            blank_finish = true;
            i += 1;
            continue;
        }
        if indent_width(next_line) < body_indent {
            break;
        }
        body_lines.push(strip_indent(next_line, body_indent).to_string());
        blank_finish = false;
        i += 1;
    }
    // Running out of input ends the item as cleanly as a blank line does.
    if i >= lines.len() {
        blank_finish = true;
    }

    while body_lines.last().is_some_and(String::is_empty) {
        body_lines.pop();
    }
    (i - start_i, body_lines, blank_finish)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_following_body_indent_reads_the_next_indented_line() {
        // Given a bare marker followed by an indented body
        let lines = ["1.", "     body", "2. next"];

        // When deriving the body indent
        let indent = following_body_indent(&lines, 0, 0);

        // Then it comes from the body line rather than a fixed marker offset
        assert_eq!(indent, 5);
    }

    #[test]
    fn test_following_body_indent_falls_back_when_nothing_is_indented() {
        // Given a bare marker with no indented line after it
        let lines = ["1.", "not indented"];

        // When deriving the body indent
        let indent = following_body_indent(&lines, 0, 0);

        // Then it falls back to just past the item indent, so the unindented
        // line is not swallowed
        assert_eq!(indent, 1);
    }

    #[test]
    fn test_collect_item_body_reports_a_blank_terminated_body_as_finished() {
        // Given an item body ended by a blank line
        let lines = ["1. a", "", "Paragraph."];
        let candidate = detect_enumerator(lines[0], None).expect("should detect");

        // When collecting the body
        let (consumed, body, blank_finish) = collect_item_body(&lines, 0, candidate, 3);

        // Then the blank line is consumed, trimmed, and reported as a clean end
        assert_eq!(consumed, 2);
        assert_eq!(body, vec!["a"]);
        assert!(blank_finish);
    }

    #[test]
    fn test_collect_item_body_reports_a_dedented_body_as_unfinished() {
        // Given an item body cut short by an unindented line
        let lines = ["1. a", "Paragraph."];
        let candidate = detect_enumerator(lines[0], None).expect("should detect");

        // When collecting the body
        let (consumed, body, blank_finish) = collect_item_body(&lines, 0, candidate, 3);

        // Then the dedent is reported so the caller can warn
        assert_eq!(consumed, 1);
        assert_eq!(body, vec!["a"]);
        assert!(!blank_finish);
    }

    #[test]
    fn test_collect_item_body_treats_end_of_input_as_a_clean_end() {
        // Given an item that runs to the end of the input
        let lines = ["1. a"];
        let candidate = detect_enumerator(lines[0], None).expect("should detect");

        // When collecting the body
        let (_, _, blank_finish) = collect_item_body(&lines, 0, candidate, 3);

        // Then it counts as finished, so no unindent warning is emitted
        assert!(blank_finish);
    }

    #[test]
    fn test_list_state_accepts_only_a_consecutive_enumerator() {
        // Given a list established as arabic, period, at ordinal one
        let first =
            Enumerator::new(EnumeratorSequence::Arabic, EnumeratorFormat::Period, 1).unwrap();
        let state = ListState {
            start: first,
            list_indent: 0,
            format: EnumeratorFormat::Period,
            sequence: EnumeratorSequence::Arabic,
            last_ordinal: 1,
            auto_engaged: false,
        };
        let candidate = |line: &str| detect_enumerator(line, Some(EnumeratorSequence::Arabic));

        // When offering it candidates
        // Then only the consecutive one in the same format and indent continues
        let next = candidate("2. b").unwrap();
        assert!(state.accepts(&next, next.enumerator.unwrap()));

        let skipped = candidate("3. c").unwrap();
        assert!(!state.accepts(&skipped, skipped.enumerator.unwrap()));

        let reformatted = candidate("2) b").unwrap();
        assert!(!state.accepts(&reformatted, reformatted.enumerator.unwrap()));

        let indented = candidate("  2. b").unwrap();
        assert!(!state.accepts(&indented, indented.enumerator.unwrap()));
    }

    #[test]
    fn test_list_state_stops_accepting_explicit_enumerators_once_auto_is_engaged() {
        // Given a list that has seen a `#` item
        let first =
            Enumerator::new(EnumeratorSequence::Arabic, EnumeratorFormat::Period, 1).unwrap();
        let state = ListState {
            start: first,
            list_indent: 0,
            format: EnumeratorFormat::Period,
            sequence: EnumeratorSequence::Arabic,
            last_ordinal: 1,
            auto_engaged: true,
        };

        // When offering it an explicit enumerator and another auto one
        let explicit = detect_enumerator("2. b", Some(EnumeratorSequence::Arabic)).unwrap();
        let auto = detect_enumerator("#. b", Some(EnumeratorSequence::Arabic)).unwrap();

        // Then only the auto one continues the list
        assert!(!state.accepts(&explicit, explicit.enumerator.unwrap()));
        assert!(state.accepts(&auto, auto.enumerator.unwrap()));
    }
}
