//! docutils' `:rfc-reference:` role, which links an RFC and nothing more —
//! no index entry and no title, though unlike `:pep-reference:` a `#`
//! section is allowed.
//!
//! Flat under `roles/` beside `pep_reference.rs`, its PEP counterpart. The
//! number is the whole of what can be wrong, so it is all this handler
//! checks; the URL is built while rendering, from the same `rfc_base_url`
//! `:rfc:` uses.

use rinx_ast::{DocutilsRfcNumber, InlineNode, RoleRefusal};

use crate::inline::escapes::unescape;
use crate::inline::regexes::RFC_REFERENCE_ROLE_REGEX;

/// Builds the `InlineNode` for a matched `:rfc-reference:` role.
///
/// The target is unescaped first, as docutils does before splitting off the
/// section and handing the rest to `int()`: there is no `Title <2822>` form,
/// so such a target is refused like any other text whose number is not at
/// least 1, and becomes an [`InlineNode::RefusedRole`] showing the role's
/// source.
pub(crate) fn handle_rfc_reference_match(m_str: &str) -> InlineNode {
    let caps = RFC_REFERENCE_ROLE_REGEX.captures(m_str).unwrap();
    let target = unescape(&caps["target"]);
    match DocutilsRfcNumber::parse(&target) {
        Ok(number) => InlineNode::DocutilsRfcReference { number, span: None },
        Err(_) => InlineNode::RefusedRole {
            text: m_str.to_string(),
            refusal: RoleRefusal::DocutilsRfcNumber { target },
            span: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(text: &str, target: &str) -> InlineNode {
        InlineNode::RefusedRole {
            text: text.to_string(),
            refusal: RoleRefusal::DocutilsRfcNumber {
                target: target.to_string(),
            },
            span: None,
        }
    }

    #[test]
    fn test_handle_rfc_reference_match_reads_a_number_and_a_section() {
        // Given / When
        let node = handle_rfc_reference_match(":rfc-reference:`2822#section-3`");

        // Then
        assert_eq!(
            node,
            InlineNode::DocutilsRfcReference {
                number: DocutilsRfcNumber::parse("2822#section-3").unwrap(),
                span: None,
            }
        );
    }

    #[test]
    fn test_handle_rfc_reference_match_refuses_an_explicit_title() {
        // Given / When
        let node = handle_rfc_reference_match(":rfc-reference:`Mail <2822>`");

        // Then
        assert_eq!(node, refused(":rfc-reference:`Mail <2822>`", "Mail <2822>"));
    }

    #[test]
    fn test_handle_rfc_reference_match_refuses_zero() {
        // Given / When
        let node = handle_rfc_reference_match(":rfc-reference:`0`");

        // Then
        assert_eq!(node, refused(":rfc-reference:`0`", "0"));
    }
}
