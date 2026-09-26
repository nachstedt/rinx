//! The two math roles: `:math:`, which holds LaTeX, and `:eq:`, which
//! references a labeled `.. math::` by its equation number.
//!
//! They sit flat under `roles/` rather than in a domain subdirectory because
//! neither belongs to a domain — Sphinx's `math` roles are available in every
//! document regardless of `default_domain`, so neither handler takes one.
//!
//! Both are deliberately the *simplest* possible handlers: neither splits an
//! explicit title. `:math:` cannot, because its content is LaTeX and
//! `` :math:`a <b>` `` is an inequality, not a display-text override; `:eq:`
//! cannot, because the text it displays is the equation's number, which no
//! single document knows until the project index is merged.

use rinx_ast::InlineNode;

use crate::inline::regexes::{EQ_ROLE_REGEX, MATH_ROLE_REGEX};

/// Builds the `InlineNode` for a matched `:math:` role.
///
/// The LaTeX is taken verbatim — no trimming, no title split — because every
/// character between the backticks is content. Escape markers are still
/// present at this point and are turned back into backslashes later, by
/// `unescape_node`'s verbatim branch.
pub(crate) fn handle_math_match(m_str: &str) -> InlineNode {
    let caps = MATH_ROLE_REGEX.captures(m_str).unwrap();
    InlineNode::Math {
        latex: caps["latex"].to_string(),
        span: None,
    }
}

/// Builds the `InlineNode` for a matched `:eq:` role.
///
/// The label *is* trimmed, unlike `:math:`'s LaTeX: it is an identifier being
/// looked up, not content, and surrounding whitespace would silently break the
/// lookup.
pub(crate) fn handle_eq_match(m_str: &str) -> InlineNode {
    let caps = EQ_ROLE_REGEX.captures(m_str).unwrap();
    InlineNode::EquationReference {
        label: caps["label"].trim().to_string(),
        span: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_math_match_keeps_latex_verbatim() {
        // Given / When
        let result = handle_math_match(r":math:`a^2 + b^2 = c^2`");

        // Then
        assert_eq!(
            result,
            InlineNode::Math {
                latex: r"a^2 + b^2 = c^2".to_string(),
                span: None,
            }
        );
    }

    #[test]
    fn test_handle_math_match_does_not_split_an_angle_bracket_as_a_title() {
        // Given LaTeX that looks like an explicit-title override but is an
        // inequality
        let result = handle_math_match(r":math:`a <b>`");

        // Then the whole thing is kept as LaTeX
        assert_eq!(
            result,
            InlineNode::Math {
                latex: r"a <b>".to_string(),
                span: None,
            }
        );
    }

    #[test]
    fn test_handle_math_match_keeps_surrounding_whitespace() {
        // Given LaTeX padded inside the backticks
        let result = handle_math_match(r":math:` x `");

        // Then it is preserved, since every character between backticks is content
        assert_eq!(
            result,
            InlineNode::Math {
                latex: r" x ".to_string(),
                span: None,
            }
        );
    }

    #[test]
    fn test_handle_eq_match_reads_the_label() {
        // Given / When
        let result = handle_eq_match(":eq:`euler`");

        // Then
        assert_eq!(
            result,
            InlineNode::EquationReference {
                label: "euler".to_string(),
                span: None,
            }
        );
    }

    #[test]
    fn test_handle_eq_match_trims_the_label() {
        // Given a label written with padding
        let result = handle_eq_match(":eq:` euler `");

        // Then the lookup key is trimmed
        assert_eq!(
            result,
            InlineNode::EquationReference {
                label: "euler".to_string(),
                span: None,
            }
        );
    }
}
