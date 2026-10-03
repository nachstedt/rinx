//! Tests for recognising well-formed section titles: what an adornment is,
//! how the two title forms are detected, and how levels are assigned.

use super::test_support::diagnostic_codes;
use super::*;
use crate::parse;
use rinx_ast::{InlineNode, TargetSearchOrder};

#[test]
fn test_is_section_adornment_accepts_single_repeated_char() {
    // Given / When / Then
    assert!(is_section_adornment("======"));
    assert!(is_section_adornment("------"));
    assert!(is_section_adornment("++++++"));
}

#[test]
fn test_is_section_adornment_rejects_mixed_grid_border() {
    // Given — a grid-table border mixes '+' and '-'
    let line = "+------+------+";

    // When / Then
    assert!(!is_section_adornment(line));
}

#[test]
fn test_is_section_adornment_rejects_empty_and_non_punctuation() {
    // Given / When / Then
    assert!(!is_section_adornment(""));
    assert!(!is_section_adornment("abc"));
}

#[test]
fn test_detect_adornment_overline() {
    let lines = vec!["#######", "Heading", "#######"];
    let result = detect_adornment(&lines, 0);
    assert!(result.is_some());
    let detected = result.unwrap();
    assert_eq!(detected.consumed, 3);
    assert_eq!(detected.adornment.character, '#');
    assert_eq!(detected.adornment.style, AdornmentStyle::Overline);
    assert_eq!(detected.text, "Heading");
    assert_eq!(detected.problem, None);
}

#[test]
fn test_detect_adornment_underline() {
    let lines = vec!["Heading", "#######"];
    let result = detect_adornment(&lines, 0);
    assert!(result.is_some());
    let detected = result.unwrap();
    assert_eq!(detected.consumed, 2);
    assert_eq!(detected.adornment.character, '#');
    assert_eq!(detected.adornment.style, AdornmentStyle::Underline);
    assert_eq!(detected.text, "Heading");
    assert_eq!(detected.problem, None);
}

#[test]
fn test_parse_creates_heading_node() {
    // Given
    let input = "Heading\n=======";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(doc.nodes.len(), 1);
    assert_eq!(
        doc.nodes[0],
        Node::Heading {
            level: 1,
            text: vec![InlineNode::Text("Heading".to_string())]
        }
    );
}

#[test]
fn test_parse_creates_heading_from_punctuation_lines() {
    // Given
    let input = "===\n---";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(doc.nodes.len(), 1);
    assert_eq!(
        doc.nodes[0],
        Node::Heading {
            level: 1,
            text: vec![InlineNode::Text("===".to_string())]
        }
    );
}

#[test]
fn test_parse_creates_h1_for_first_adornment_char() {
    // Given — a single heading using `=`
    let input = "Title\n=====";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(doc.nodes.len(), 1);
    assert_eq!(
        doc.nodes[0],
        Node::Heading {
            level: 1,
            text: vec![InlineNode::Text("Title".to_string())]
        }
    );
}

#[test]
fn test_parse_creates_h2_for_second_adornment_char() {
    // Given — first heading with `=`, second with `-`
    let input = "H1\n==\n\nH2\n--";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(doc.nodes.len(), 2);
    assert_eq!(
        doc.nodes[0],
        Node::Heading {
            level: 1,
            text: vec![InlineNode::Text("H1".to_string())]
        }
    );
    assert_eq!(
        doc.nodes[1],
        Node::Heading {
            level: 2,
            text: vec![InlineNode::Text("H2".to_string())]
        }
    );
}

#[test]
fn test_parse_reuses_level_for_same_adornment_char() {
    // Given — both headings use the same `=` adornment
    let input = "First\n=====\n\nSecond\n======";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(doc.nodes.len(), 2);
    assert_eq!(
        doc.nodes[0],
        Node::Heading {
            level: 1,
            text: vec![InlineNode::Text("First".to_string())]
        }
    );
    assert_eq!(
        doc.nodes[1],
        Node::Heading {
            level: 1,
            text: vec![InlineNode::Text("Second".to_string())]
        }
    );
}

#[test]
fn test_parse_assigns_levels_by_encounter_order() {
    // Given — three headings using `=`, `-`, and `~` in that order
    let input = "H1\n==\n\nH2\n--\n\nH3\n~~";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(doc.nodes.len(), 3);
    assert_eq!(
        doc.nodes[0],
        Node::Heading {
            level: 1,
            text: vec![InlineNode::Text("H1".to_string())]
        }
    );
    assert_eq!(
        doc.nodes[1],
        Node::Heading {
            level: 2,
            text: vec![InlineNode::Text("H2".to_string())]
        }
    );
    assert_eq!(
        doc.nodes[2],
        Node::Heading {
            level: 3,
            text: vec![InlineNode::Text("H3".to_string())]
        }
    );
}

#[test]
fn test_parse_creates_heading_for_alternate_punctuation() {
    // Given
    let input = "Sub Title\n---------";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(doc.nodes.len(), 1);
    assert_eq!(
        doc.nodes[0],
        Node::Heading {
            level: 1,
            text: vec![InlineNode::Text("Sub Title".to_string())]
        }
    );
}

#[test]
fn test_parse_creates_heading_with_overline() {
    // Given
    let input = "#######\nHeading\n#######";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(doc.nodes.len(), 1);
    assert_eq!(
        doc.nodes[0],
        Node::Heading {
            level: 1,
            text: vec![InlineNode::Text("Heading".to_string())]
        }
    );
}

#[test]
fn test_parse_overline_and_underline_distinct_levels() {
    // Given — same char '#' but different styles
    let input = "##########\nOverline\n##########\n\nUnderline\n#########";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(doc.nodes.len(), 2);
    assert_eq!(
        doc.nodes[0],
        Node::Heading {
            level: 1,
            text: vec![InlineNode::Text("Overline".to_string())]
        }
    );
    assert_eq!(
        doc.nodes[1],
        Node::Heading {
            level: 2,
            text: vec![InlineNode::Text("Underline".to_string())]
        }
    );
}

#[test]
fn test_parse_heading_resolves_domain_object_role() {
    // Given
    let input = "The :mod:`greetings` Module\n============================";

    // When
    let doc = crate::parse_with_domain("test.rst", input, rinx_ast::Domain::Py);

    // Then
    assert_eq!(doc.nodes.len(), 1);
    assert_eq!(
        doc.nodes[0],
        Node::Heading {
            level: 1,
            text: vec![
                InlineNode::Text("The ".to_string()),
                InlineNode::DomainObjectReference {
                    object_type: rinx_ast::ObjectType::Py(rinx_ast::PyObjectType::Module),
                    name: "greetings".to_string(),
                    display: "greetings".to_string(),
                    link: true,
                    search_order: TargetSearchOrder::LeastQualifiedFirst,
                    // `The ` is four characters, and the role is sixteen —
                    // a heading's text is mapped like any other line.
                    span: Some(rinx_ast::Span::new(
                        rinx_ast::Position::new(1, 5),
                        rinx_ast::Position::new(1, 21),
                    )),
                    inventory: rinx_ast::InventorySelector::Any,
                },
                InlineNode::Text(" Module".to_string()),
            ]
        }
    );
}

#[test]
fn test_parse_heading_resolves_strong_emphasis() {
    // Given
    let input = "A **Bold** Heading\n===================";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(doc.nodes.len(), 1);
    assert_eq!(
        doc.nodes[0],
        Node::Heading {
            level: 1,
            text: vec![
                InlineNode::Text("A ".to_string()),
                InlineNode::Strong("Bold".to_string()),
                InlineNode::Text(" Heading".to_string()),
            ]
        }
    );
}

/// The codes every diagnostic the parse reported carries, in order.
#[test]
fn test_parse_heading_measures_an_emoji_title_in_display_columns() {
    // Given — the shape every page title of the sphinx-needs demo corpus
    // has: an emoji two columns wide but four bytes long, under an
    // underline drawn to the title's *visible* width.
    let input = "\u{1f50d} Demo details\n===============";

    // When
    let doc = parse("test.rst", input);

    // Then — a heading, and nothing to report: 15 columns under 15 `=`
    assert_eq!(
        doc.nodes,
        vec![Node::Heading {
            level: 1,
            text: vec![InlineNode::Text("\u{1f50d} Demo details".to_string())]
        }]
    );
    assert!(diagnostic_codes(&doc).is_empty());
}

#[test]
fn test_parse_heading_requires_two_columns_per_wide_character() {
    // Given — a CJK title of two characters, hence four columns
    let input = "\u{6f22}\u{5b57}\n====";

    // When
    let doc = parse("test.rst", input);

    // Then — the four-character underline is exactly wide enough
    assert_eq!(
        doc.nodes,
        vec![Node::Heading {
            level: 1,
            text: vec![InlineNode::Text("\u{6f22}\u{5b57}".to_string())]
        }]
    );
    assert!(diagnostic_codes(&doc).is_empty());
}

#[test]
fn test_parse_heading_measures_an_overlined_emoji_title_in_display_columns() {
    // Given — the same title in the three-line form
    let input = "===============\n\u{1f50d} Demo details\n===============";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(
        doc.nodes,
        vec![Node::Heading {
            level: 1,
            text: vec![InlineNode::Text("\u{1f50d} Demo details".to_string())]
        }]
    );
    assert!(diagnostic_codes(&doc).is_empty());
}

#[test]
fn test_parse_heading_accepts_a_short_underline_and_reports_it() {
    // Given — a title wider than its underline, which is still four
    // characters or more
    let input = "A Long Title\n=====";

    // When
    let doc = parse("test.rst", input);

    // Then — docutils' behaviour: still a heading, reported rather than
    // silently dropped
    assert_eq!(
        doc.nodes,
        vec![Node::Heading {
            level: 1,
            text: vec![InlineNode::Text("A Long Title".to_string())]
        }]
    );
    assert_eq!(
        diagnostic_codes(&doc),
        vec![DiagnosticCode::HeadingUnderlineTooShort]
    );
}

#[test]
fn test_parse_heading_reports_a_short_overlined_adornment_once() {
    // Given — the three-line form, both adornments too short
    let input = "=====\nA Long Title\n=====";

    // When
    let doc = parse("test.rst", input);

    // Then — one heading and exactly one diagnostic, not one per adornment
    assert_eq!(
        doc.nodes,
        vec![Node::Heading {
            level: 1,
            text: vec![InlineNode::Text("A Long Title".to_string())]
        }]
    );
    assert_eq!(
        diagnostic_codes(&doc),
        vec![DiagnosticCode::HeadingUnderlineTooShort]
    );
}

#[test]
fn test_parse_leaves_a_title_over_a_very_short_adornment_as_text() {
    // Given — an underline of fewer than four characters under a wider
    // title, which docutils declines to read as a heading
    let input = "A Long Title\n~~~";

    // When
    let doc = parse("test.rst", input);

    // Then — ordinary text, and deliberately no diagnostic: the pattern
    // matches far more prose than it does mis-drawn headings
    assert_eq!(
        doc.nodes,
        vec![Node::Paragraph(vec![InlineNode::Text(
            "A Long Title\n~~~".to_string()
        )])]
    );
    assert!(diagnostic_codes(&doc).is_empty());
}

#[test]
fn test_parse_heading_reports_nothing_for_an_over_long_underline() {
    // Given — an underline wider than the title
    let input = "Short\n==========";

    // When
    let doc = parse("test.rst", input);

    // Then — a heading, with nothing to report
    assert_eq!(
        doc.nodes,
        vec![Node::Heading {
            level: 1,
            text: vec![InlineNode::Text("Short".to_string())]
        }]
    );
    assert!(diagnostic_codes(&doc).is_empty());
}

#[test]
fn test_adornment_fits_distinguishes_the_three_outcomes() {
    // Given / When / Then — wide enough, tolerably short, too short
    assert_eq!(adornment_fits("Title", "====="), AdornmentFit::Covers);
    assert_eq!(
        adornment_fits("A Long Title", "===="),
        AdornmentFit::TooShort
    );
    assert_eq!(adornment_fits("A Long Title", "==="), AdornmentFit::Refused);
}

#[test]
fn test_adornment_fits_measures_the_title_in_columns_not_bytes() {
    // Given — a title whose byte length exceeds its display width
    let text = "\u{1f50d} Demo details";

    // When / Then — an underline matching the display width fits, and the
    // byte length is not what is asked about
    assert_eq!(adornment_fits(text, &"=".repeat(15)), AdornmentFit::Covers);
    assert_eq!(text.len(), 17);
}
