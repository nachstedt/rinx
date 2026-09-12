use super::inline::{SourceMap, parse_inline_text_mapped};
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::width::column_width;
use rusty_sphinx_ast::{Diagnostic, DiagnosticCode, Node};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AdornmentStyle {
    Underline,
    Overline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Adornment {
    pub(super) character: char,
    pub(super) style: AdornmentStyle,
}

/// Checks whether `line` is a valid section adornment: a non-empty run of a
/// *single* repeated ASCII punctuation character (e.g. `=====`, `-----`,
/// `+++++`). Mixed-punctuation lines such as a grid-table border
/// (`+------+------+`) are deliberately rejected, matching the RST spec's
/// definition of section adornments and mirroring `try_parse_transition`'s
/// single-character rule — this is what keeps a header-less grid table from
/// being mistaken for an overline/underline heading.
pub(super) fn is_section_adornment(line: &str) -> bool {
    let mut chars = line.chars();
    match chars.next() {
        Some(first) if first.is_ascii_punctuation() => chars.all(|c| c == first),
        _ => false,
    }
}

/// The shortest adornment that may still carry a title wider than itself.
///
/// docutils' own threshold: below it, a short run of punctuation under a line
/// of prose is far likelier to be something else than a heading the author
/// mis-drew, so the pair degrades to ordinary text.
const MIN_TOLERATED_ADORNMENT: usize = 4;

/// A heading recognised at some line, and what was wrong with it.
///
/// A named struct rather than a tuple because the last field answers a
/// different question from the first three: they say *what* was parsed, it
/// says what to report about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DetectedHeading {
    /// Lines the heading occupies: 3 for the overlined form, 2 otherwise.
    pub(super) consumed: usize,
    pub(super) adornment: Adornment,
    pub(super) text: String,
    /// The adornment is shorter than the title's display width. docutils
    /// accepts such a heading and warns, rather than rejecting it, as long as
    /// the adornment is at least [`MIN_TOLERATED_ADORNMENT`] characters.
    pub(super) underline_too_short: bool,
}

/// Decides whether `adornment` may underline a title of `text`, and whether
/// that is worth reporting.
///
/// The title is measured in **display columns**, never in bytes: an author
/// draws the underline under what they see, so a byte comparison rejects every
/// heading whose title holds a non-ASCII character — which is what made the
/// emoji-prefixed titles of the sphinx-needs demo corpus parse as paragraphs.
///
/// `None` means this is not a heading at all. Note the deliberate silence
/// there: docutils additionally emits an info-level `possible title underline,
/// too short for the title` for the too-short case, which this build cannot
/// express (it has no severity below warning) and should not promote to a
/// warning — measured over the `CPython` corpus the rule fires nine times at
/// block level, every one of them a literal-block `::` marker rather than a
/// heading anybody mis-drew.
fn adornment_fits(text: &str, adornment: &str) -> Option<bool> {
    if column_width(text) <= adornment.len() {
        Some(false)
    } else if adornment.len() >= MIN_TOLERATED_ADORNMENT {
        Some(true)
    } else {
        None
    }
}

pub(super) fn detect_adornment(lines: &[&str], i: usize) -> Option<DetectedHeading> {
    // 1. Try 3-line pattern (Overline + Text + Underline)
    if i + 2 < lines.len() {
        let overline = lines[i].trim();
        let text = lines[i + 1].trim();
        let underline = lines[i + 2].trim();

        if is_section_adornment(overline)
            && overline == underline
            && let Some(underline_too_short) = adornment_fits(text, overline)
        {
            let adornment_char = overline.chars().next().expect("non-empty adornment");
            return Some(DetectedHeading {
                consumed: 3,
                adornment: Adornment {
                    character: adornment_char,
                    style: AdornmentStyle::Overline,
                },
                text: text.to_string(),
                underline_too_short,
            });
        }
    }

    // 2. Try 2-line pattern (Text + Underline)
    if i + 1 < lines.len() {
        let text = lines[i].trim();
        let underline = lines[i + 1].trim();

        if is_section_adornment(underline)
            && let Some(underline_too_short) = adornment_fits(text, underline)
        {
            let adornment_char = underline.chars().next().expect("non-empty underline");
            return Some(DetectedHeading {
                consumed: 2,
                adornment: Adornment {
                    character: adornment_char,
                    style: AdornmentStyle::Underline,
                },
                text: text.to_string(),
                underline_too_short,
            });
        }
    }

    None
}

pub(super) fn try_parse_heading(
    lines: &[&str],
    i: usize,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<(usize, Node)> {
    let DetectedHeading {
        consumed,
        adornment,
        text,
        underline_too_short,
    } = detect_adornment(lines, i)?;
    // The heading's *text* line, which is the one below the overline in the
    // three-line form and the line itself in the two-line form.
    let text_line = match adornment.style {
        AdornmentStyle::Overline => i + 1,
        AdornmentStyle::Underline => i,
    };

    if underline_too_short {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::HeadingUnderlineTooShort,
            "section title is wider than the adornment underlining it",
            ctx.line_span(text_line, lines[text_line]),
        ));
    }

    let level = adornment_order
        .iter()
        .position(|&a| a == adornment)
        .map_or_else(
            || {
                adornment_order.push(adornment);
                adornment_order.len()
            },
            |pos| pos + 1,
        );

    #[allow(clippy::cast_possible_truncation)]
    let level = level as u8;

    let text = parse_inline_text_mapped(
        &text,
        ctx.default_domain,
        &SourceMap::single_line(
            &text,
            text_line,
            lines[text_line]
                .chars()
                .take_while(|c| c.is_whitespace())
                .count(),
        ),
        ctx,
    );

    Some((consumed, Node::Heading { level, text }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rusty_sphinx_ast::{InlineNode, TargetSearchOrder};

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
        assert!(!detected.underline_too_short);
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
        assert!(!detected.underline_too_short);
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
    fn test_parse_overline_requires_exact_match_with_underline() {
        // Given — mismatched overline/underline length
        let input = "#######\nHeading\n######";

        // When
        let doc = parse("test.rst", input);

        // Then — the three-line form is rejected, so the overline stays
        // ordinary text; the two lines below it are then read on their own as
        // an underlined heading whose adornment is too short.
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![InlineNode::Text("#######".to_string())])
        );
        assert_eq!(
            doc.nodes[1],
            Node::Heading {
                level: 1,
                text: vec![InlineNode::Text("Heading".to_string())]
            }
        );
        assert_eq!(
            doc.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            vec![DiagnosticCode::HeadingUnderlineTooShort]
        );
    }

    #[test]
    fn test_parse_heading_resolves_domain_object_role() {
        // Given
        let input = "The :mod:`greetings` Module\n============================";

        // When
        let doc = crate::parse_with_domain("test.rst", input, rusty_sphinx_ast::Domain::Py);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Heading {
                level: 1,
                text: vec![
                    InlineNode::Text("The ".to_string()),
                    InlineNode::DomainObjectReference {
                        object_type: rusty_sphinx_ast::ObjectType::Py(
                            rusty_sphinx_ast::PyObjectType::Module
                        ),
                        name: "greetings".to_string(),
                        display: "greetings".to_string(),
                        link: true,
                        search_order: TargetSearchOrder::LeastQualifiedFirst,
                        // `The ` is four characters, and the role is sixteen —
                        // a heading's text is mapped like any other line.
                        span: Some(rusty_sphinx_ast::Span::new(
                            rusty_sphinx_ast::Position::new(1, 5),
                            rusty_sphinx_ast::Position::new(1, 21),
                        )),
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
    fn diagnostic_codes(doc: &rusty_sphinx_ast::Document) -> Vec<DiagnosticCode> {
        doc.diagnostics.iter().map(|d| d.code).collect()
    }

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
        assert_eq!(adornment_fits("Title", "====="), Some(false));
        assert_eq!(adornment_fits("A Long Title", "===="), Some(true));
        assert_eq!(adornment_fits("A Long Title", "==="), None);
    }

    #[test]
    fn test_adornment_fits_measures_the_title_in_columns_not_bytes() {
        // Given — a title whose byte length exceeds its display width
        let text = "\u{1f50d} Demo details";

        // When / Then — an underline matching the display width fits, and the
        // byte length is not what is asked about
        assert_eq!(adornment_fits(text, &"=".repeat(15)), Some(false));
        assert_eq!(text.len(), 17);
    }
}
