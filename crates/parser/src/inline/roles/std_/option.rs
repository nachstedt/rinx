use rinx_ast::InlineNode;

use crate::explicit_title::split_display_and_target;
use crate::inline::regexes::OPTION_ROLE_REGEX;

/// Builds the `InlineNode` for a matched `:option:` role. Unlike the
/// `py`/`c` domain-object roles, `:option:` does no per-domain resolution —
/// there is only one `std`-domain object type it can mean — so, unlike
/// [`super::super::py::func::handle_func_match`] and friends, this takes no
/// `default_domain` parameter.
pub(crate) fn handle_option_match(m_str: &str) -> InlineNode {
    let caps = OPTION_ROLE_REGEX.captures(m_str).unwrap();
    let (display, target) = split_display_and_target(&caps["content"]);
    InlineNode::OptionReference {
        display,
        target,
        span: None,
        inventory: rinx_ast::InventorySelector::Any,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_option_match_basic() {
        let result = handle_option_match(":option:`-m`");
        assert_eq!(
            result,
            InlineNode::OptionReference {
                display: "-m".to_string(),
                target: "-m".to_string(),
                span: None,
                inventory: rinx_ast::InventorySelector::Any,
            }
        );
    }
    #[test]
    fn test_handle_option_match_explicit_title_splits_display_from_target() {
        let result = handle_option_match(":option:`the module flag <-m>`");
        assert_eq!(
            result,
            InlineNode::OptionReference {
                display: "the module flag".to_string(),
                target: "-m".to_string(),
                span: None,
                inventory: rinx_ast::InventorySelector::Any,
            }
        );
    }
}
