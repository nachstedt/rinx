//! docutils' `:pep-reference:` role, which links a Python Enhancement
//! Proposal and nothing more — no index entry, no title, no fragment.
//!
//! Flat under `roles/` beside `pep.rs`, its Sphinx sibling. The number is the
//! whole of what can be wrong, so it is all this handler checks; the URL is
//! built while rendering, from the same `pep_base_url` `:pep:` uses.

use rinx_ast::{DocutilsPepNumber, InlineNode, RoleRefusal};

use crate::inline::escapes::unescape;
use crate::inline::regexes::PEP_REFERENCE_ROLE_REGEX;

/// Builds the `InlineNode` for a matched `:pep-reference:` role.
///
/// The whole target is the number, unescaped first as docutils does before
/// handing it to `int()`: there is no `Title <8>` form, so such a target is
/// refused like any other text that is not a number from 0 to 9999, and
/// becomes an [`InlineNode::RefusedRole`] showing the role's source.
pub(crate) fn handle_pep_reference_match(m_str: &str) -> InlineNode {
    let caps = PEP_REFERENCE_ROLE_REGEX.captures(m_str).unwrap();
    let target = unescape(&caps["target"]);
    match DocutilsPepNumber::parse(&target) {
        Ok(number) => InlineNode::DocutilsPepReference { number, span: None },
        Err(_) => InlineNode::RefusedRole {
            text: m_str.to_string(),
            refusal: RoleRefusal::DocutilsPepNumber { target },
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
            refusal: RoleRefusal::DocutilsPepNumber {
                target: target.to_string(),
            },
            span: None,
        }
    }

    #[test]
    fn test_handle_pep_reference_match_reads_a_number() {
        // Given / When
        let node = handle_pep_reference_match(":pep-reference:`08`");

        // Then
        assert_eq!(
            node,
            InlineNode::DocutilsPepReference {
                number: DocutilsPepNumber::parse("08").unwrap(),
                span: None,
            }
        );
    }

    #[test]
    fn test_handle_pep_reference_match_refuses_an_explicit_title() {
        // Given / When
        let node = handle_pep_reference_match(":pep-reference:`Style <8>`");

        // Then
        assert_eq!(node, refused(":pep-reference:`Style <8>`", "Style <8>"));
    }

    #[test]
    fn test_handle_pep_reference_match_refuses_a_fragment() {
        // Given / When
        let node = handle_pep_reference_match(":pep-reference:`8#naming`");

        // Then
        assert_eq!(node, refused(":pep-reference:`8#naming`", "8#naming"));
    }

    #[test]
    fn test_handle_pep_reference_match_refuses_a_number_out_of_range() {
        // Given / When
        let node = handle_pep_reference_match(":pep-reference:`10000`");

        // Then
        assert_eq!(node, refused(":pep-reference:`10000`", "10000"));
    }
}
