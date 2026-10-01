//! `:sub:`/`:subscript:` and `:sup:`/`:superscript:`, docutils' generic roles
//! for text set below or above the line.
//!
//! Flat under `roles/` for the reason `:math:` is: they belong to no domain.
//! Their content is plain text, as docutils gives a generic role — no nested
//! markup and no explicit-title split, so `` :sup:`a <b>` `` is that text.
//! Unlike `:code:` it is prose, though, so smart typography applies to it as
//! it does to emphasis. Escape markers are still present here;
//! `unescape_node` drops them later.

use rinx_ast::{InlineNode, ScriptPosition};

use crate::inline::regexes::SCRIPT_ROLE_REGEX;
use crate::inline::typography::apply_smart_typography;

/// Builds the `InlineNode` for a matched `:sub:`, `:subscript:`, `:sup:` or
/// `:superscript:` role, carrying no classes of its own.
pub(crate) fn handle_script_match(m_str: &str) -> InlineNode {
    let caps = SCRIPT_ROLE_REGEX.captures(m_str).unwrap();
    let position = ScriptPosition::from_role_name(&caps["role"])
        .expect("the regex matches only the four script role names");
    script_node(position, &caps["text"], Vec::new())
}

/// The node a script role — built in or derived with `.. role::` — shows
/// `text` as.
pub(crate) fn script_node(
    position: ScriptPosition,
    text: &str,
    classes: Vec<String>,
) -> InlineNode {
    InlineNode::Script {
        position,
        text: apply_smart_typography(text),
        classes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script(position: ScriptPosition, text: &str) -> InlineNode {
        InlineNode::Script {
            position,
            text: text.to_string(),
            classes: Vec::new(),
        }
    }

    #[test]
    fn test_handle_script_match_reads_every_spelling() {
        // Given / When / Then
        assert_eq!(
            handle_script_match(":sub:`2`"),
            script(ScriptPosition::Subscript, "2")
        );
        assert_eq!(
            handle_script_match(":subscript:`2`"),
            script(ScriptPosition::Subscript, "2")
        );
        assert_eq!(
            handle_script_match(":sup:`2`"),
            script(ScriptPosition::Superscript, "2")
        );
        assert_eq!(
            handle_script_match(":superscript:`2`"),
            script(ScriptPosition::Superscript, "2")
        );
    }

    #[test]
    fn test_handle_script_match_keeps_surrounding_whitespace() {
        // Given text padded inside the backticks
        let result = handle_script_match(":sup:` th `");

        // Then every character between the backticks is content
        assert_eq!(result, script(ScriptPosition::Superscript, " th "));
    }

    #[test]
    fn test_handle_script_match_does_not_split_an_angle_bracket_as_a_title() {
        // Given text that looks like an explicit-title override
        let result = handle_script_match(":sub:`a <b>`");

        // Then the whole thing is the text
        assert_eq!(result, script(ScriptPosition::Subscript, "a <b>"));
    }

    #[test]
    fn test_script_node_applies_smart_typography() {
        // Given / When
        let result = script_node(ScriptPosition::Superscript, "1--2", Vec::new());

        // Then
        assert_eq!(result, script(ScriptPosition::Superscript, "1\u{2013}2"));
    }

    #[test]
    fn test_script_node_keeps_the_classes() {
        // Given / When
        let result = script_node(ScriptPosition::Subscript, "2", vec!["chem".to_string()]);

        // Then
        assert_eq!(
            result,
            InlineNode::Script {
                position: ScriptPosition::Subscript,
                text: "2".to_string(),
                classes: vec!["chem".to_string()],
            }
        );
    }
}
