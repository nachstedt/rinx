//! The `:pep:` role, which links a Python Enhancement Proposal and indexes
//! the place it was mentioned.
//!
//! Flat under `roles/` for the reason `download.rs` is: it names something
//! outside every domain — here a page outside the site altogether. The one
//! part of the role that can be wrong on its own is the number, so that is
//! what this handler checks; the anchor its index entry links to is minted
//! once the whole document is parsed, and the URL only while rendering.

use rinx_ast::{InlineNode, PepTarget, RoleRefusal};

use crate::explicit_title::split_optional_title;
use crate::inline::escapes::unescape;
use crate::inline::regexes::PEP_ROLE_REGEX;

/// Builds the `InlineNode` for a matched `:pep:` role.
///
/// The target is unescaped before it is read as a number, since Sphinx hands
/// `int()` the text docutils already unescaped. A target that names no number
/// becomes an [`InlineNode::RefusedRole`] showing the role's source, which
/// the whole-document pass reports at this role's span. There is no `!`
/// form: Sphinx's `PEP` role has none, and `!8` is not a number.
pub(crate) fn handle_pep_match(m_str: &str) -> InlineNode {
    let caps = PEP_ROLE_REGEX.captures(m_str).unwrap();
    let (display, target) = split_optional_title(&caps["target"]);
    let target = unescape(&target);
    match PepTarget::parse(&target) {
        Ok(target) => InlineNode::PepReference {
            target,
            display,
            index_id: String::new(),
            span: None,
        },
        Err(_) => InlineNode::RefusedRole {
            text: m_str.to_string(),
            refusal: RoleRefusal::PepTarget { target },
            span: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pep(target: &str, display: Option<&str>) -> InlineNode {
        InlineNode::PepReference {
            target: PepTarget::parse(target).unwrap(),
            display: display.map(str::to_string),
            index_id: String::new(),
            span: None,
        }
    }

    #[test]
    fn test_handle_pep_match_reads_a_bare_number() {
        // Given / When
        let node = handle_pep_match(":pep:`8`");

        // Then
        assert_eq!(node, pep("8", None));
    }

    #[test]
    fn test_handle_pep_match_keeps_a_fragment_in_the_target() {
        // Given / When
        let node = handle_pep_match(":pep:`8#naming-conventions`");

        // Then
        assert_eq!(node, pep("8#naming-conventions", None));
    }

    #[test]
    fn test_handle_pep_match_splits_an_explicit_title() {
        // Given / When
        let node = handle_pep_match(":pep:`Style guide <8>`");

        // Then
        assert_eq!(node, pep("8", Some("Style guide")));
    }

    #[test]
    fn test_handle_pep_match_refuses_a_target_that_is_no_number() {
        // Given / When
        let node = handle_pep_match(":pep:`Style <abc>`");

        // Then — shown as the role's source, and the target kept for the
        // diagnostic
        assert_eq!(
            node,
            InlineNode::RefusedRole {
                text: ":pep:`Style <abc>`".to_string(),
                refusal: RoleRefusal::PepTarget {
                    target: "abc".to_string(),
                },
                span: None,
            }
        );
    }

    #[test]
    fn test_handle_pep_match_has_no_bang_form() {
        // Given / When
        let node = handle_pep_match(":pep:`!8`");

        // Then
        assert!(matches!(node, InlineNode::RefusedRole { .. }), "{node:?}");
    }
}
