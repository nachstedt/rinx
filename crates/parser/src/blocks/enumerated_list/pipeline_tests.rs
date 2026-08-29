//! End-to-end `parse()` tests for enumerated lists: every sequence and
//! format, the continuation rules, nesting, and the diagnostics. Kept
//! separate from [`super::list`]'s own unit tests purely for file size.

use crate::parse;
use rusty_sphinx_ast::{
    Document, Enumerator, EnumeratorFormat, EnumeratorSequence, ListItem, Node, inline_plain_text,
};

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
