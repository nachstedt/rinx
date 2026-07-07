use super::inline::parse_inline_text;
use rusty_sphinx_ast::{Domain, Node};

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

pub(super) fn detect_adornment(lines: &[&str], i: usize) -> Option<(usize, Adornment, String)> {
    // 1. Try 3-line pattern (Overline + Text + Underline)
    if i + 2 < lines.len() {
        let overline = lines[i].trim();
        let text = lines[i + 1].trim();
        let underline = lines[i + 2].trim();

        if !overline.is_empty()
            && overline.chars().all(|c| c.is_ascii_punctuation())
            && overline == underline
            && overline.len() >= text.len()
        {
            let adornment_char = overline.chars().next().expect("non-empty adornment");
            return Some((
                3,
                Adornment {
                    character: adornment_char,
                    style: AdornmentStyle::Overline,
                },
                text.to_string(),
            ));
        }
    }

    // 2. Try 2-line pattern (Text + Underline)
    if i + 1 < lines.len() {
        let text = lines[i].trim();
        let underline = lines[i + 1].trim();

        if !underline.is_empty()
            && underline.chars().all(|c| c.is_ascii_punctuation())
            && underline.len() >= text.len()
        {
            let adornment_char = underline.chars().next().expect("non-empty underline");
            return Some((
                2,
                Adornment {
                    character: adornment_char,
                    style: AdornmentStyle::Underline,
                },
                text.to_string(),
            ));
        }
    }

    None
}

pub(super) fn try_parse_heading(
    lines: &[&str],
    i: usize,
    adornment_order: &mut Vec<Adornment>,
    default_domain: Domain,
) -> Option<(usize, Node)> {
    let (consumed, adornment, text) = detect_adornment(lines, i)?;

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

    let text = parse_inline_text(&text, default_domain);

    Some((consumed, Node::Heading { level, text }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rusty_sphinx_ast::InlineNode;

    #[test]
    fn test_detect_adornment_overline() {
        let lines = vec!["#######", "Heading", "#######"];
        let result = detect_adornment(&lines, 0);
        assert!(result.is_some());
        let (consumed, adornment, text) = result.unwrap();
        assert_eq!(consumed, 3);
        assert_eq!(adornment.character, '#');
        assert_eq!(adornment.style, AdornmentStyle::Overline);
        assert_eq!(text, "Heading");
    }

    #[test]
    fn test_detect_adornment_underline() {
        let lines = vec!["Heading", "#######"];
        let result = detect_adornment(&lines, 0);
        assert!(result.is_some());
        let (consumed, adornment, text) = result.unwrap();
        assert_eq!(consumed, 2);
        assert_eq!(adornment.character, '#');
        assert_eq!(adornment.style, AdornmentStyle::Underline);
        assert_eq!(text, "Heading");
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

        // Then — should not be a heading
        assert_eq!(doc.nodes.len(), 1);
        match &doc.nodes[0] {
            Node::Paragraph(_) => {}
            _ => panic!("Expected paragraph for mismatched overline/underline"),
        }
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
}
