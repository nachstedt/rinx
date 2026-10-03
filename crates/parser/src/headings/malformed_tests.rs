//! Tests for section titles docutils rejects: an overline that its underline
//! does not match, an overline with no underline, two adornments with no
//! title between them, and adornments that do not start in column 0.

use super::test_support::diagnostic_codes;
use super::*;
use crate::parse;
use rinx_ast::InlineNode;

fn headings(doc: &rinx_ast::Document) -> Vec<(u8, Vec<InlineNode>)> {
    doc.nodes
        .iter()
        .filter_map(|node| match node {
            Node::Heading { level, text } => Some((*level, text.clone())),
            _ => None,
        })
        .collect()
}

fn title(text: &str) -> Vec<InlineNode> {
    vec![InlineNode::Text(text.to_string())]
}

#[test]
fn test_parse_keeps_a_title_whose_underline_is_longer_than_its_overline() {
    // Given
    let input = "=======\nTitle\n==========\n";

    // When
    let doc = parse("test.rst", input);

    // Then — one heading, and no stray paragraph holding the overline
    assert_eq!(
        doc.nodes,
        vec![Node::Heading {
            level: 1,
            text: title("Title")
        }]
    );
    assert_eq!(
        diagnostic_codes(&doc),
        vec![DiagnosticCode::HeadingOverlineMismatch]
    );
}

#[test]
fn test_parse_keeps_a_title_whose_underline_is_another_character() {
    // Given
    let input = "=======\nTitle\n-------\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(headings(&doc), vec![(1, title("Title"))]);
    assert_eq!(
        diagnostic_codes(&doc),
        vec![DiagnosticCode::HeadingOverlineMismatch]
    );
}

#[test]
fn test_parse_reports_only_the_mismatch_for_a_wide_title() {
    // Given — the title is wider than both adornments, but docutils stops at
    // the mismatch before it measures the title
    let input = "==========\nA Long Title Here\n=====\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(headings(&doc), vec![(1, title("A Long Title Here"))]);
    assert_eq!(
        diagnostic_codes(&doc),
        vec![DiagnosticCode::HeadingOverlineMismatch]
    );
}

#[test]
fn test_parse_names_both_adornments_in_the_mismatch_message() {
    // Given
    let input = "=======\nTitle\n----------\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    let message = &doc.diagnostics[0].message;
    assert!(message.contains("7 × '='"), "{message}");
    assert!(message.contains("10 × '-'"), "{message}");
}

#[test]
fn test_parse_spans_all_three_lines_of_a_mismatched_title() {
    // Given
    let input = "Intro.\n\n=======\nTitle\n==========\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    let span = doc.diagnostics[0].span.expect("a positioned diagnostic");
    assert_eq!((span.start.line, span.end.line), (3, 5));
}

#[test]
fn test_parse_gives_a_mismatched_title_its_overline_style() {
    // Given — the second title repeats the first one's overline style
    let input = "Top\n===\n\n-------\nFirst\n-------\n\n-------\nSecond\n----------\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(
        headings(&doc),
        vec![(1, title("Top")), (2, title("First")), (2, title("Second"))]
    );
}

#[test]
fn test_parse_treats_a_short_mismatched_overline_as_text() {
    // Given — below four characters an overline is ordinary text, as in
    // docutils, and the lines below it make an underlined title
    let input = "===\nTitle\n=====\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(headings(&doc), vec![(1, title("Title"))]);
    assert_eq!(diagnostic_codes(&doc), Vec::new());
}

#[test]
fn test_parse_reports_an_overline_without_an_underline() {
    // Given
    let input = "=======\nTitle\n\nText.\n";

    // When
    let doc = parse("test.rst", input);

    // Then — the lines stay text
    assert!(headings(&doc).is_empty(), "{:?}", doc.nodes);
    assert_eq!(
        diagnostic_codes(&doc),
        vec![DiagnosticCode::HeadingMissingUnderline]
    );
    let span = doc.diagnostics[0].span.expect("a positioned diagnostic");
    assert_eq!(span.start.line, 1);
}

#[test]
fn test_parse_reports_an_overline_and_title_at_the_end_of_the_input() {
    // Given
    let input = "Intro.\n\n=======\nTitle";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert!(headings(&doc).is_empty(), "{:?}", doc.nodes);
    assert_eq!(
        diagnostic_codes(&doc),
        vec![DiagnosticCode::HeadingMissingUnderline]
    );
}

#[test]
fn test_parse_reports_an_overline_directly_above_prose() {
    // Given — a rule missing its blank line reads as an overline
    let input = "Intro.\n\n--------\nSome text.\nMore text.\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert!(headings(&doc).is_empty(), "{:?}", doc.nodes);
    assert_eq!(
        diagnostic_codes(&doc),
        vec![DiagnosticCode::HeadingMissingUnderline]
    );
}

#[test]
fn test_parse_reports_two_adornments_with_no_title_between_them() {
    // Given
    let input = "Intro.\n\n=====\n-----\n\nText.\n";

    // When
    let doc = parse("test.rst", input);

    // Then — no heading titled `=====`
    assert!(headings(&doc).is_empty(), "{:?}", doc.nodes);
    assert_eq!(
        diagnostic_codes(&doc),
        vec![DiagnosticCode::HeadingAdornmentWithoutTitle]
    );
}

#[test]
fn test_parse_reports_an_adornment_between_two_others() {
    // Given — what looks like a title is itself an adornment
    let input = "Intro.\n\n=====\n-----\n=====\n\nText.\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert!(headings(&doc).is_empty(), "{:?}", doc.nodes);
    assert_eq!(
        diagnostic_codes(&doc),
        vec![DiagnosticCode::HeadingAdornmentWithoutTitle]
    );
}

#[test]
fn test_parse_reports_nothing_for_a_transition() {
    // Given
    let input = "Intro.\n\n-----\n\nMore.\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert!(doc.nodes.contains(&Node::Transition));
    assert_eq!(diagnostic_codes(&doc), Vec::new());
}

#[test]
fn test_parse_does_not_underline_with_an_indented_adornment() {
    // Given — docutils reads this as a definition list, not a title
    let input = "Term\n   -----\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert!(headings(&doc).is_empty(), "{:?}", doc.nodes);
}

#[test]
fn test_parse_accepts_an_inset_title_between_column_zero_adornments() {
    // Given — only the title of an overlined heading may be inset
    let input = "=========\n  Title\n=========\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(headings(&doc), vec![(1, title("Title"))]);
    assert_eq!(diagnostic_codes(&doc), Vec::new());
}

#[test]
fn test_find_malformed_overline_finds_a_missing_underline() {
    // Given / When / Then
    assert_eq!(
        find_malformed_overline(&["=====", "Title", ""], 0),
        Some(MalformedOverline::MissingUnderline)
    );
    assert_eq!(
        find_malformed_overline(&["=====", "Title"], 0),
        Some(MalformedOverline::MissingUnderline)
    );
    assert_eq!(
        find_malformed_overline(&["=====", "Title", "   ====="], 0),
        Some(MalformedOverline::MissingUnderline)
    );
}

#[test]
fn test_find_malformed_overline_finds_an_adornment_without_title() {
    // Given / When / Then
    assert_eq!(
        find_malformed_overline(&["=====", "-----"], 0),
        Some(MalformedOverline::AdornmentWithoutTitle)
    );
}

#[test]
fn test_find_malformed_overline_leaves_well_formed_and_short_lines_alone() {
    // Given / When / Then — an underline below the title is a heading's business
    assert_eq!(find_malformed_overline(&["=====", "Title", "---"], 0), None);
    // a blank line below is a transition's
    assert_eq!(find_malformed_overline(&["=====", ""], 0), None);
    assert_eq!(find_malformed_overline(&["====="], 0), None);
    // a short or indented first line is not an overline at all
    assert_eq!(find_malformed_overline(&["===", "Title"], 0), None);
    assert_eq!(find_malformed_overline(&["  =====", "Title"], 0), None);
    assert_eq!(find_malformed_overline(&["Title", "====="], 0), None);
}

#[test]
fn test_column_zero_adornment_rejects_an_indented_line() {
    // Given / When / Then
    assert_eq!(column_zero_adornment("=====  "), Some("====="));
    assert_eq!(column_zero_adornment("  ====="), None);
    assert_eq!(column_zero_adornment("Title"), None);
}

#[test]
fn test_is_long_adornment_needs_four_characters() {
    // Given / When / Then
    assert!(is_long_adornment("===="));
    assert!(!is_long_adornment("==="));
    assert!(!is_long_adornment("Text"));
}

#[test]
fn test_heading_level_registers_a_new_style_only_where_titles_are_allowed() {
    // Given
    let top = Adornment {
        character: '=',
        style: AdornmentStyle::Underline,
    };
    let other = Adornment {
        character: '-',
        style: AdornmentStyle::Underline,
    };
    let mut order = vec![top];

    // When
    let forbidden = heading_level(other, &mut order, SectionTitles::Forbidden);
    let known = heading_level(top, &mut order, SectionTitles::Forbidden);
    let allowed = heading_level(other, &mut order, SectionTitles::Allowed);

    // Then
    assert_eq!((forbidden, known, allowed), (2, 1, 2));
    assert_eq!(order, vec![top, other]);
}

#[test]
fn test_overline_mismatch_message_counts_characters() {
    // Given / When
    let message = overline_mismatch_message("=====  ", "~~~");

    // Then
    assert!(message.contains("5 × '='"), "{message}");
    assert!(message.contains("3 × '~'"), "{message}");
}

#[test]
fn test_adornment_fit_problem_maps_each_fit() {
    // Given / When / Then
    assert_eq!(AdornmentFit::Covers.problem(), Ok(None));
    assert_eq!(
        AdornmentFit::TooShort.problem(),
        Ok(Some(HeadingProblem::AdornmentTooShort))
    );
    assert_eq!(AdornmentFit::Refused.problem(), Err(()));
}

#[test]
fn test_detect_overlined_title_needs_both_adornments_in_column_zero() {
    // Given / When / Then
    assert!(detect_overlined_title(&["=====", "Title", "====="], 0).is_some());
    assert!(detect_overlined_title(&["=====", "  Title", "====="], 0).is_some());
    assert!(detect_overlined_title(&[" =====", "Title", "====="], 0).is_none());
    assert!(detect_overlined_title(&["=====", "Title", " ====="], 0).is_none());
    assert!(detect_overlined_title(&["=====", "", "====="], 0).is_none());
    assert!(detect_overlined_title(&["=====", "Title"], 0).is_none());
}

#[test]
fn test_detect_overlined_title_reports_a_mismatch_only_for_a_long_overline() {
    // Given / When
    let long = detect_overlined_title(&["=====", "Title", "======="], 0);
    let short = detect_overlined_title(&["===", "Title", "====="], 0);

    // Then
    assert_eq!(
        long.map(|heading| heading.problem),
        Some(Some(HeadingProblem::OverlineMismatch))
    );
    assert_eq!(short, None);
}

#[test]
fn test_detect_underlined_title_refuses_a_long_adornment_as_its_title() {
    // Given / When / Then
    assert!(detect_underlined_title(&["Title", "====="], 0).is_some());
    assert!(detect_underlined_title(&["=====", "-----"], 0).is_none());
    assert!(detect_underlined_title(&["...", "====="], 0).is_some());
    assert!(detect_underlined_title(&["  Title", "====="], 0).is_none());
    assert!(detect_underlined_title(&["Title", "  ====="], 0).is_none());
}
