//! Enumerated (ordered) list parsing.
//!
//! This is a port of docutils' `states.py` — `Body.parse_enumerator`,
//! `Body.is_enumerated_list_item`, `Body.make_enumerator` and the
//! `EnumeratedList` specialized state — rather than a simplified "line starts
//! with a digit" detector, because the spec's rules are load-bearing:
//!
//! * An enumerator is ambiguous. `v.` is the 22nd *letter*, not roman five; only
//!   a bare `i.`/`I.` seeds a roman list. Once a list's sequence is established,
//!   later items are read in that sequence first, which is what keeps
//!   `A. B. … H. I.` counting letters instead of flipping to roman at `I`.
//! * A line that looks like an enumerator is only an item if the *next* line
//!   agrees — blank, more-indented, or carrying the next enumerator in the
//!   sequence. This is what keeps `A. Einstein said this.` / `He was smart.` a
//!   paragraph. Note it only bites in that exact shape: with a blank line, an
//!   indented continuation, or at end of input, `A. Einstein` really does
//!   become a one-item list, in docutils too. The spec's advice there is to
//!   escape the period.
//! * Any break in format, sequence or ordering starts a *new* list rather than
//!   continuing the current one, so an `EnumeratedList` node never has to
//!   record per-item enumerators.
//!
//! Diagnostics go beyond docutils'. Its two messages are reproduced verbatim
//! (the not-ordinal-1 info and the unexpected-unindent warning), and
//! [`diagnose_unrecognised_list`] adds one docutils lacks: when two adjacent
//! lines both carry an enumerator but the continuation rule rejects them, the
//! entire block silently becomes a paragraph, which is close to impossible to
//! diagnose from the rendered output. That message names the cause. It stays
//! silent across every file of the benchmark corpus (see `docs/benchmark.md`),
//! so it is precise enough not to need narrowing.
//!
//! One deliberate behavioural difference from [`super::bullet_list`]: when a
//! list is interrupted mid-item by an unindented line, docutils drops the
//! already-scanned item back into the input rather than keeping it, so
//! `1. a` / `2. b` / `Paragraph.` yields a *one*-item list followed by the
//! paragraph `2. b\nParagraph.`. Bullet lists keep both items. That asymmetry
//! is docutils', and [`try_parse_enumerated_list`] reproduces it by testing
//! each item before consuming it.

use super::blocks::{indent_width, parse_blocks};
use super::bullet_list::strip_indent;
use super::headings::Adornment;
use rusty_sphinx_ast::{Domain, Enumerator, EnumeratorFormat, EnumeratorSequence, ListItem, Node};

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
pub(super) fn try_parse_enumerated_list(
    lines: &[&str],
    start_i: usize,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
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
        if !is_enumerated_list_item(lines, i, enumerator, candidate.is_auto) {
            if state.is_none() {
                diagnose_unrecognised_list(lines, i, &candidate, enumerator, diagnostics);
            }
            break;
        }

        let body_indent = candidate
            .body_indent
            .unwrap_or_else(|| following_body_indent(lines, i, candidate.item_indent));

        let (consumed, body_lines, item_blank_finish) =
            collect_item_body(lines, i, candidate, body_indent);
        i += consumed;
        blank_finish = item_blank_finish;

        let body_refs: Vec<&str> = body_lines.iter().map(String::as_str).collect();
        let nodes = parse_blocks(&body_refs, adornment_order, diagnostics, default_domain);
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
        diagnostics.push(format!(
            "Enumerated list start value not ordinal-1: \"{}\" (ordinal {})",
            state.start.marker_text(),
            state.start.ordinal()
        ));
    }
    if !blank_finish {
        diagnostics
            .push("Enumerated list ends without a blank line; unexpected unindent.".to_string());
    }

    Some((
        i - start_i,
        Node::EnumeratedList {
            start: state.start,
            items,
        },
    ))
}

/// Why two adjacent enumerators do not form one list.
///
/// Every variant describes a case where the author plainly wrote a list — two
/// consecutive lines both carrying an enumerator, at the same indent — but the
/// spec's continuation rule rejects it, so the whole block degrades to a
/// paragraph. That outcome is silent in docutils; naming the cause is a
/// deliberate addition, because "my list rendered as prose" is otherwise very
/// hard to diagnose from the output alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListBreakReason {
    /// A `#` item cannot be followed by an explicit enumerator.
    AutoFollowedByExplicit,
    /// `1.` then `1)`.
    DifferentFormat,
    /// `1.` then `a.`.
    DifferentSequence,
    /// `1.` then `3.`.
    NonConsecutive,
    /// `1.` then `002.` — the right ordinal, spelled uncanonically.
    NonCanonicalSpelling,
}

impl ListBreakReason {
    /// The clause naming this cause inside the diagnostic.
    fn describe(self) -> &'static str {
        match self {
            Self::AutoFollowedByExplicit => {
                "an automatic enumerator cannot be followed by an explicit one"
            }
            Self::DifferentFormat => "different enumerator format",
            Self::DifferentSequence => "different enumeration sequence",
            Self::NonConsecutive => "not the next enumerator in the sequence",
            Self::NonCanonicalSpelling => "not the canonical spelling of its ordinal",
        }
    }
}

/// Classifies why `second` does not continue `first`.
///
/// Returns `None` when the two *would* have formed a list, which means the
/// rejection came from something this classification does not model; staying
/// silent is better than guessing.
fn classify_list_break(
    first: Enumerator,
    first_is_auto: bool,
    second: Enumerator,
    second_is_auto: bool,
    second_text: &str,
) -> Option<ListBreakReason> {
    if first_is_auto && !second_is_auto {
        return Some(ListBreakReason::AutoFollowedByExplicit);
    }
    if first.format() != second.format() {
        return Some(ListBreakReason::DifferentFormat);
    }
    if first.sequence() != second.sequence() {
        return Some(ListBreakReason::DifferentSequence);
    }
    if Some(second.ordinal()) != first.ordinal().checked_add(1) {
        return Some(ListBreakReason::NonConsecutive);
    }
    // Same sequence, format and position, yet the successor check failed: the
    // marker must be spelled differently from its canonical form, e.g. `002.`
    // for ordinal 2.
    (second_text != second.to_string()).then_some(ListBreakReason::NonCanonicalSpelling)
}

/// Reports a block that looks like an enumerated list but is not one.
///
/// Only fires when the following line also carries a valid enumerator at the
/// same indent. That condition is what keeps ordinary prose quiet: in
/// `A. Einstein said this.` / `He was smart.` the second line is not an
/// enumerator, so nothing is reported.
fn diagnose_unrecognised_list(
    lines: &[&str],
    i: usize,
    candidate: &EnumeratorMatch,
    enumerator: Enumerator,
    diagnostics: &mut Vec<String>,
) {
    let Some(next_line) = lines.get(i + 1).map(|l| l.trim_end()) else {
        return;
    };
    let Some(next) = detect_enumerator(next_line, Some(enumerator.sequence())) else {
        return;
    };
    let Some(next_enumerator) = next.enumerator else {
        return;
    };
    if next.item_indent != candidate.item_indent {
        return;
    }

    let next_text = next_line.split_whitespace().next().unwrap_or_default();
    let Some(reason) = classify_list_break(
        enumerator,
        candidate.is_auto,
        next_enumerator,
        next.is_auto,
        next_text,
    ) else {
        return;
    };

    let first_text = if candidate.is_auto {
        format!(
            "{}#{}",
            enumerator.format().prefix(),
            enumerator.format().suffix()
        )
    } else {
        enumerator.to_string()
    };
    diagnostics.push(format!(
        "Enumerated list not recognised: \"{first_text}\" is followed by \"{next_text}\" ({}), so the block was parsed as a paragraph. Separate adjacent lists with a blank line, or renumber so the enumerators run consecutively.",
        reason.describe()
    ));
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
/// trimmed off the end — the same shape as [`super::bullet_list`]'s loop.
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
    use crate::parse;
    use rusty_sphinx_ast::{Document, inline_plain_text};

    /// Returns the single enumerated list a document is expected to contain.
    fn only_list(doc: &Document) -> (&Enumerator, &[ListItem]) {
        match doc.nodes.as_slice() {
            [Node::EnumeratedList { start, items }] => (start, items),
            other => panic!("Expected exactly one EnumeratedList, got {other:?}"),
        }
    }

    /// Returns the plain text of an item that holds exactly one paragraph.
    fn item_text(item: &ListItem) -> String {
        match item.nodes.as_slice() {
            [Node::Paragraph(inlines)] => inline_plain_text(inlines),
            other => panic!("Expected a single paragraph, got {other:?}"),
        }
    }

    #[test]
    fn test_parses_every_sequence_in_every_format() {
        // Given the first two enumerators of each sequence, in each format
        let sequences = [
            (EnumeratorSequence::Arabic, "1", "2"),
            (EnumeratorSequence::LowerAlpha, "a", "b"),
            (EnumeratorSequence::UpperAlpha, "A", "B"),
            (EnumeratorSequence::LowerRoman, "i", "ii"),
            (EnumeratorSequence::UpperRoman, "I", "II"),
        ];

        for (sequence, first, second) in sequences {
            for format in EnumeratorFormat::PROBE_ORDER {
                let marker = |text: &str| format!("{}{}{}", format.prefix(), text, format.suffix());
                let input = format!("{} one\n{} two\n", marker(first), marker(second));

                // When parsing the list
                let doc = parse("test.rst", &input);

                // Then the sequence, format and both items are recognised
                let (start, items) = only_list(&doc);
                assert_eq!(start.sequence(), sequence, "{input:?}");
                assert_eq!(start.format(), format, "{input:?}");
                assert_eq!(start.ordinal(), 1, "{input:?}");
                assert_eq!(items.len(), 2, "{input:?}");
                assert!(
                    doc.diagnostics.is_empty(),
                    "{input:?} {:?}",
                    doc.diagnostics
                );
            }
        }
    }

    #[test]
    fn test_parses_an_auto_enumerated_list() {
        // Given a list written entirely with the `#` auto-enumerator
        let input = "#. one\n#. two\n#. three\n";

        // When parsing it
        let doc = parse("test.rst", input);

        // Then it becomes an arabic list starting at one, as docutils records it
        let (start, items) = only_list(&doc);
        assert_eq!(start.sequence(), EnumeratorSequence::Arabic);
        assert_eq!(start.ordinal(), 1);
        assert_eq!(items.len(), 3);
        assert!(doc.diagnostics.is_empty());
    }

    #[test]
    fn test_parses_an_explicit_first_item_followed_by_auto_enumerators() {
        // Given a list that numbers its first item and defers the rest
        let input = "1. one\n#. two\n#. three\n";

        // When parsing it
        let doc = parse("test.rst", input);

        // Then all three items belong to one list
        let (start, items) = only_list(&doc);
        assert_eq!(start.ordinal(), 1);
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn test_an_explicit_enumerator_after_an_auto_one_does_not_continue_the_list() {
        // Given an auto-enumerated item followed by an explicit one
        let input = "#. one\n2. two\n";

        // When parsing it
        let doc = parse("test.rst", input);

        // Then no list forms at all: the successor check fails on line one, so
        // both lines stay prose — docutils reaches the same outcome
        assert!(matches!(doc.nodes.as_slice(), [Node::Paragraph(_)]));
    }

    #[test]
    fn test_records_a_start_value_other_than_one() {
        // Given a list that begins partway through its sequence
        let input = "3. three\n4. four\n";

        // When parsing it
        let doc = parse("test.rst", input);

        // Then the start ordinal is kept and reported, as docutils does
        let (start, items) = only_list(&doc);
        assert_eq!(start.ordinal(), 3);
        assert_eq!(items.len(), 2);
        assert!(
            doc.diagnostics
                .iter()
                .any(|d| d == "Enumerated list start value not ordinal-1: \"3\" (ordinal 3)"),
            "{:?}",
            doc.diagnostics
        );
    }

    #[test]
    fn test_a_bare_v_starts_a_lower_alpha_list_not_a_roman_one() {
        // Given the ambiguous enumerator `v`, which is both the 22nd letter and
        // roman five
        let input = "v. five?\n";

        // When parsing it
        let doc = parse("test.rst", input);

        // Then it resolves to lower-alpha, because docutils' resolution order
        // reaches alpha before roman and only seeds roman from `i`/`I`
        let (start, _) = only_list(&doc);
        assert_eq!(start.sequence(), EnumeratorSequence::LowerAlpha);
        assert_eq!(start.ordinal(), 22);
    }

    #[test]
    fn test_a_bare_i_seeds_a_lower_roman_list() {
        // Given a list opening with `i`
        let input = "i. one\nii. two\n";

        // When parsing it
        let doc = parse("test.rst", input);

        // Then roman is chosen, the one sequence docutils special-cases
        let (start, items) = only_list(&doc);
        assert_eq!(start.sequence(), EnumeratorSequence::LowerRoman);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_an_established_alpha_list_reads_i_as_a_letter() {
        // Given an upper-alpha list long enough to reach `I`
        let input = "A. a\nB. b\nC. c\nD. d\nE. e\nF. f\nG. g\nH. h\nI. i\n";

        // When parsing it
        let doc = parse("test.rst", input);

        // Then `I` continues the alphabet rather than restarting as roman one —
        // this is what the expected-sequence hint exists for
        let (start, items) = only_list(&doc);
        assert_eq!(start.sequence(), EnumeratorSequence::UpperAlpha);
        assert_eq!(items.len(), 9);
        assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
    }

    #[test]
    fn test_an_arabic_list_followed_by_i_does_not_continue() {
        // Given an arabic item followed by `i.`
        let input = "1. one\ni. two\n";

        // When parsing it
        let doc = parse("test.rst", input);

        // Then the successor check fails on item one and both lines stay prose.
        // Note the roman seeds are unreachable here: docutils puts them in an
        // `elif` after the expected-sequence branch.
        assert!(matches!(doc.nodes.as_slice(), [Node::Paragraph(_)]));
    }

    #[test]
    fn test_prose_beginning_with_an_initial_is_not_a_list() {
        // Given a sentence opening with an initial, continued on the next line
        let input = "A. Einstein said this.\nHe was smart.\n";

        // When parsing it
        let doc = parse("test.rst", input);

        // Then it stays a paragraph, because the following line neither is
        // blank nor indented nor carries `B. `
        assert!(matches!(doc.nodes.as_slice(), [Node::Paragraph(_)]));
    }

    #[test]
    fn test_an_initial_at_end_of_input_really_is_a_one_item_list() {
        // Given the same initial with nothing following it
        let input = "A. Einstein said this.\n";

        // When parsing it
        let doc = parse("test.rst", input);

        // Then it *is* a list — docutils agrees, which is why the spec tells
        // authors to escape the period. The ambiguity rule only bites when a
        // non-blank, unindented line follows.
        let (start, items) = only_list(&doc);
        assert_eq!(start.sequence(), EnumeratorSequence::UpperAlpha);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn test_a_broken_sequence_does_not_continue_the_list() {
        // Given lists whose second line breaks the format, the ordering, or the
        // required trailing space after the marker
        let inputs = [
            "1. a\n1) b\n",
            "1. a\n3. c\n",
            "1. a\n2.\n",
            "1. a\nb. two\n",
        ];

        for input in inputs {
            // When parsing each
            let doc = parse("test.rst", input);

            // Then no list forms: the successor check rejects line one, and the
            // whole block falls through to a paragraph
            assert!(
                matches!(doc.nodes.as_slice(), [Node::Paragraph(_)]),
                "{input:?} produced {:?}",
                doc.nodes
            );
        }
    }

    #[test]
    fn test_a_list_interrupted_by_prose_keeps_only_its_finished_items() {
        // Given two items followed immediately by an unindented paragraph
        let input = "1. a\n2. b\nParagraph.\n";

        // When parsing it
        let doc = parse("test.rst", input);

        // Then only the first item survives as a list and the rest is prose —
        // docutils drops the interrupted item back into the input, so bullet
        // lists and enumerated lists genuinely differ on this shape
        match doc.nodes.as_slice() {
            [Node::EnumeratedList { items, .. }, Node::Paragraph(inlines)] => {
                assert_eq!(items.len(), 1);
                assert_eq!(inline_plain_text(inlines), "2. b\nParagraph.");
            }
            other => panic!("Expected a one-item list then a paragraph, got {other:?}"),
        }

        // And the unindent is reported
        assert!(
            doc.diagnostics
                .iter()
                .any(|d| d == "Enumerated list ends without a blank line; unexpected unindent."),
            "{:?}",
            doc.diagnostics
        );
    }

    #[test]
    fn test_a_list_ending_at_a_blank_line_is_not_reported_as_unindented() {
        // Given a list separated from the following paragraph by a blank line
        let input = "1. a\n\nParagraph.\n";

        // When parsing it
        let doc = parse("test.rst", input);

        // Then no unindent warning is emitted
        assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
    }

    #[test]
    fn test_parses_a_nested_list_inside_an_item() {
        // Given an enumerated list whose first item contains an indented one
        let input = "1. outer\n\n   a. inner\n   b. inner two\n\n2. next\n";

        // When parsing it
        let doc = parse("test.rst", input);

        // Then the inner list is a child of the first item
        let (_, items) = only_list(&doc);
        assert_eq!(items.len(), 2);
        match items[0].nodes.as_slice() {
            [
                Node::Paragraph(_),
                Node::EnumeratedList {
                    start,
                    items: inner,
                },
            ] => {
                assert_eq!(start.sequence(), EnumeratorSequence::LowerAlpha);
                assert_eq!(inner.len(), 2);
            }
            other => panic!("Expected a paragraph then a nested list, got {other:?}"),
        }
    }

    #[test]
    fn test_parses_an_enumerated_list_nested_in_a_bullet_list() {
        // Given a bullet item containing an enumerated list
        let input = "* outer\n\n  1. one\n  2. two\n";

        // When parsing it
        let doc = parse("test.rst", input);

        // Then the enumerated list is nested inside the bullet item
        match doc.nodes.as_slice() {
            [Node::BulletList { items, .. }] => match items[0].nodes.as_slice() {
                [
                    Node::Paragraph(_),
                    Node::EnumeratedList { items: inner, .. },
                ] => {
                    assert_eq!(inner.len(), 2);
                }
                other => panic!("Expected a nested enumerated list, got {other:?}"),
            },
            other => panic!("Expected a bullet list, got {other:?}"),
        }
    }

    #[test]
    fn test_parses_an_item_spanning_several_paragraphs() {
        // Given an item whose body has two blank-separated paragraphs
        let input = "1. first para\n\n   second para\n\n2. next\n";

        // When parsing it
        let doc = parse("test.rst", input);

        // Then both paragraphs belong to the first item
        let (_, items) = only_list(&doc);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].nodes.len(), 2);
        assert!(matches!(items[0].nodes[0], Node::Paragraph(_)));
        assert!(matches!(items[0].nodes[1], Node::Paragraph(_)));
    }

    #[test]
    fn test_parses_an_item_whose_marker_is_alone_on_its_line() {
        // Given an item whose body starts on the line after the marker, at an
        // indent the marker width alone would not predict
        let input = "1.\n     body here\n2. next\n";

        // When parsing it
        let doc = parse("test.rst", input);

        // Then the body indent is taken from the following line, so the text is
        // not left with stray leading spaces
        let (_, items) = only_list(&doc);
        assert_eq!(items.len(), 2);
        assert_eq!(item_text(&items[0]), "body here");
    }

    #[test]
    fn test_parses_a_directive_inside_an_item() {
        // Given an item containing an admonition
        let input = "1. intro\n\n   .. note::\n\n      Careful.\n\n2. next\n";

        // When parsing it
        let doc = parse("test.rst", input);

        // Then the directive is parsed as a child of the item, since item
        // bodies go back through the full block dispatch
        let (_, items) = only_list(&doc);
        assert!(
            items[0]
                .nodes
                .iter()
                .any(|n| matches!(n, Node::Directive(_))),
            "{:?}",
            items[0].nodes
        );
    }

    #[test]
    fn test_a_decimal_number_is_not_an_enumerator() {
        // Given a sentence opening with a decimal number
        let input = "1.5 is a number\n";

        // When parsing it
        let doc = parse("test.rst", input);

        // Then it stays prose: the marker must be followed by a space or the
        // end of the line
        assert!(matches!(doc.nodes.as_slice(), [Node::Paragraph(_)]));
    }

    #[test]
    fn test_a_roman_numeral_beyond_the_representable_range_is_not_an_enumerator() {
        // Given roman text shaped correctly but denoting no ordinal
        let input = "mmmmm. x\n";

        // When parsing it
        let doc = parse("test.rst", input);

        // Then it stays prose, matching docutils' `ordinal is None` path
        assert!(matches!(doc.nodes.as_slice(), [Node::Paragraph(_)]));
    }

    #[test]
    fn test_leading_zeros_are_normalised_when_continuing_a_list() {
        // Given an item numbered with leading zeros
        let input = "007. a\n8. b\n";

        // When parsing it
        let doc = parse("test.rst", input);

        // Then the ordinal is decoded numerically, so `8.` continues the list
        let (start, items) = only_list(&doc);
        assert_eq!(start.ordinal(), 7);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_a_list_does_not_interrupt_the_paragraph_above_it() {
        // Given a paragraph immediately followed by a list, with no blank line
        let input = "Some text\n1. Item\n";

        // When parsing it
        let doc = parse("test.rst", input);

        // Then both lines are one paragraph. docutils behaves identically —
        // its text block reader stops only at blank or indented lines — and
        // bullet lists already work this way, so this is not a bug to "fix"
        assert!(matches!(doc.nodes.as_slice(), [Node::Paragraph(_)]));
    }

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

    /// Parses `input` and returns only the "not recognised" diagnostics.
    fn unrecognised_diagnostics(input: &str) -> Vec<String> {
        parse("test.rst", input)
            .diagnostics
            .into_iter()
            .filter(|d| d.starts_with("Enumerated list not recognised"))
            .collect()
    }

    #[test]
    fn test_reports_each_way_two_enumerators_fail_to_form_a_list() {
        // Given blocks whose two lines both carry an enumerator but which the
        // continuation rule rejects, one per cause
        let cases = [
            ("1. a\n1) b\n", "different enumerator format"),
            ("1. a\na. b\n", "different enumeration sequence"),
            ("1. a\n3. b\n", "not the next enumerator in the sequence"),
            (
                "#. a\n2. b\n",
                "an automatic enumerator cannot be followed by an explicit one",
            ),
            (
                "1. a\n002. b\n",
                "not the canonical spelling of its ordinal",
            ),
        ];

        for (input, reason) in cases {
            // When parsing each
            let diagnostics = unrecognised_diagnostics(input);

            // Then exactly one diagnostic names the cause, so the author is not
            // left wondering why their list rendered as prose
            assert_eq!(diagnostics.len(), 1, "{input:?} -> {diagnostics:?}");
            assert!(
                diagnostics[0].contains(reason),
                "{input:?} -> {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_does_not_report_prose_that_merely_starts_like_an_enumerator() {
        // Given prose whose first line opens with an initial
        let input = "A. Einstein said this.\nHe was smart.\n";

        // When parsing it
        let diagnostics = unrecognised_diagnostics(input);

        // Then nothing is reported: the second line carries no enumerator, so
        // there is no reason to think a list was intended
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn test_does_not_report_a_list_that_parses_successfully() {
        // Given well-formed lists
        for input in ["1. a\n2. b\n", "#. a\n#. b\n", "1. a\n#. b\n"] {
            // When parsing them
            let diagnostics = unrecognised_diagnostics(input);

            // Then nothing is reported
            assert!(diagnostics.is_empty(), "{input:?} -> {diagnostics:?}");
        }
    }

    #[test]
    fn test_does_not_report_a_lone_enumerator_at_end_of_input() {
        // Given a single enumerator line with nothing after it
        let input = "5. only item\n";

        // When parsing it
        let diagnostics = unrecognised_diagnostics(input);

        // Then nothing is reported — it parses as a one-item list, so there is
        // no failure to explain
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn test_classify_list_break_names_each_cause() {
        // Given an arabic period enumerator at ordinal one
        let first =
            Enumerator::new(EnumeratorSequence::Arabic, EnumeratorFormat::Period, 1).unwrap();
        let other = |sequence, format, ordinal| Enumerator::new(sequence, format, ordinal).unwrap();

        // When classifying each kind of mismatch
        // Then the cause is identified
        assert_eq!(
            classify_list_break(
                first,
                true,
                other(EnumeratorSequence::Arabic, EnumeratorFormat::Period, 2),
                false,
                "2."
            ),
            Some(ListBreakReason::AutoFollowedByExplicit)
        );
        assert_eq!(
            classify_list_break(
                first,
                false,
                other(EnumeratorSequence::Arabic, EnumeratorFormat::RightParen, 2),
                false,
                "2)"
            ),
            Some(ListBreakReason::DifferentFormat)
        );
        assert_eq!(
            classify_list_break(
                first,
                false,
                other(EnumeratorSequence::LowerAlpha, EnumeratorFormat::Period, 2),
                false,
                "b."
            ),
            Some(ListBreakReason::DifferentSequence)
        );
        assert_eq!(
            classify_list_break(
                first,
                false,
                other(EnumeratorSequence::Arabic, EnumeratorFormat::Period, 3),
                false,
                "3."
            ),
            Some(ListBreakReason::NonConsecutive)
        );
        assert_eq!(
            classify_list_break(
                first,
                false,
                other(EnumeratorSequence::Arabic, EnumeratorFormat::Period, 2),
                false,
                "002."
            ),
            Some(ListBreakReason::NonCanonicalSpelling)
        );
    }

    #[test]
    fn test_classify_list_break_stays_silent_when_the_pair_would_have_formed_a_list() {
        // Given two enumerators that do continue one another
        let first =
            Enumerator::new(EnumeratorSequence::Arabic, EnumeratorFormat::Period, 1).unwrap();
        let second =
            Enumerator::new(EnumeratorSequence::Arabic, EnumeratorFormat::Period, 2).unwrap();

        // When classifying them
        let reason = classify_list_break(first, false, second, false, "2.");

        // Then no cause is reported, rather than a guessed one
        assert_eq!(reason, None);
    }

    #[test]
    fn test_classify_list_break_does_not_overflow_at_the_top_of_the_arabic_range() {
        // Given a first enumerator at the largest representable ordinal
        let first = Enumerator::new(
            EnumeratorSequence::Arabic,
            EnumeratorFormat::Period,
            u32::MAX,
        )
        .unwrap();
        let second =
            Enumerator::new(EnumeratorSequence::Arabic, EnumeratorFormat::Period, 1).unwrap();

        // When classifying against any successor
        let reason = classify_list_break(first, false, second, false, "1.");

        // Then it reports a break rather than overflowing while computing one
        assert_eq!(reason, Some(ListBreakReason::NonConsecutive));
    }

    #[test]
    fn test_diagnose_unrecognised_list_stays_silent_at_a_different_indent() {
        // Given two enumerators that disagree but sit at different indents,
        // where the mismatch is nesting rather than a broken list
        let lines = ["1. a", "  1) b"];
        let candidate = detect_enumerator(lines[0], None).expect("should detect");
        let mut diagnostics = Vec::new();

        // When diagnosing
        diagnose_unrecognised_list(
            &lines,
            0,
            &candidate,
            candidate.enumerator.unwrap(),
            &mut diagnostics,
        );

        // Then nothing is reported
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn test_list_break_reason_descriptions_are_distinct() {
        // Given every cause
        let reasons = [
            ListBreakReason::AutoFollowedByExplicit,
            ListBreakReason::DifferentFormat,
            ListBreakReason::DifferentSequence,
            ListBreakReason::NonConsecutive,
            ListBreakReason::NonCanonicalSpelling,
        ];

        // When collecting their descriptions
        let mut described: Vec<_> = reasons.iter().map(|r| r.describe()).collect();
        described.sort_unstable();
        described.dedup();

        // Then each cause reads differently in the diagnostic
        assert_eq!(described.len(), reasons.len());
    }
}
