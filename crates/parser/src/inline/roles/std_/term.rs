//! The `:term:` role, which links a glossary term.

use rinx_ast::{InlineNode, InventorySelector};

use crate::explicit_title::split_display_and_target;
use crate::inline::regexes::TERM_ROLE_REGEX;
use crate::inline::roles::spec::{ExternalPrefix, RoleSpec};

/// The role this module reads, as the role table lists it.
pub(crate) const SPEC: RoleSpec = RoleSpec {
    names: &["term"],
    domains: &[],
    external: ExternalPrefix::Accepted,
    doc: "Link a term of a glossary.",
};

/// Builds the `InlineNode` for a matched `:term:` role. Whether a glossary
/// defines the term is decided while rendering.
pub(crate) fn handle_term_match(m_str: &str) -> InlineNode {
    let caps = TERM_ROLE_REGEX.captures(m_str).unwrap();
    let (display, term) = split_display_and_target(&caps["content"]);
    InlineNode::TermReference {
        display,
        term,
        span: None,
        inventory: InventorySelector::Any,
    }
}
