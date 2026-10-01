//! `:code:`. The roles a document derives from it with
//! `.. role:: name(code)` are built by [`super::custom`].
//!
//! Flat under `roles/` for the reason `:math:` is: it belongs to no domain.
//! It keeps every character between the backticks — no trimming and no
//! explicit-title split, since `` :code:`a <b>` `` is code, not a title.
//! Escape markers are still present here; `unescape_node` drops them later,
//! taking the display form because Sphinx's `code_role` receives interpreted
//! text rather than a literal's verbatim one.

use rinx_ast::{InlineNode, ResolvedLanguage};

use crate::inline::regexes::CODE_ROLE_REGEX;

/// Builds the `InlineNode` for a matched `:code:` role: unhighlighted, with
/// no classes of its own.
pub(crate) fn handle_code_match(m_str: &str) -> InlineNode {
    let caps = CODE_ROLE_REGEX.captures(m_str).unwrap();
    InlineNode::Code {
        text: caps["code"].to_string(),
        language: ResolvedLanguage::None,
        classes: Vec::new(),
        span: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(text: &str) -> InlineNode {
        InlineNode::Code {
            text: text.to_string(),
            language: ResolvedLanguage::None,
            classes: Vec::new(),
            span: None,
        }
    }

    #[test]
    fn test_handle_code_match_reads_the_text() {
        // Given / When
        let result = handle_code_match(":code:`x = 1`");

        // Then
        assert_eq!(result, code("x = 1"));
    }

    #[test]
    fn test_handle_code_match_keeps_surrounding_whitespace() {
        // Given code padded inside the backticks
        let result = handle_code_match(":code:` x `");

        // Then every character between the backticks is content
        assert_eq!(result, code(" x "));
    }

    #[test]
    fn test_handle_code_match_does_not_split_an_angle_bracket_as_a_title() {
        // Given code that looks like an explicit-title override
        let result = handle_code_match(":code:`a <b>`");

        // Then the whole thing is code
        assert_eq!(result, code("a <b>"));
    }
}
