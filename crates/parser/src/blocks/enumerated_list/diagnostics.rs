use super::format::{EnumeratorMatch, detect_enumerator};
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use rusty_sphinx_ast::{Diagnostic, DiagnosticCode, Enumerator};

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
pub(super) fn diagnose_unrecognised_list(
    lines: &[&str],
    i: usize,
    candidate: &EnumeratorMatch,
    enumerator: Enumerator,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
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
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::ListEnumeratedNotRecognised,
        format!(
            "Enumerated list not recognised: \"{first_text}\" is followed by \"{next_text}\" ({}), so the block was parsed as a paragraph. Separate adjacent lists with a blank line, or renumber so the enumerators run consecutively.",
            reason.describe()
        ),
        // Both lines, since the diagnostic is about the pair: neither is wrong
        // on its own, only their succession is.
        ctx.lines_span(i, i + 1, next_line),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rusty_sphinx_ast::Domain;
    use rusty_sphinx_ast::{EnumeratorFormat, EnumeratorSequence};

    /// Parses `input` and returns only the "not recognised" diagnostics.
    fn unrecognised_diagnostics(input: &str) -> Vec<String> {
        parse("test.rst", input)
            .diagnostics
            .into_iter()
            .filter(|d| d.message.starts_with("Enumerated list not recognised"))
            .map(|d| d.message)
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
        let mut diagnostics = Diagnostics::default();

        // When diagnosing
        diagnose_unrecognised_list(
            &lines,
            0,
            &candidate,
            candidate.enumerator.unwrap(),
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
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
