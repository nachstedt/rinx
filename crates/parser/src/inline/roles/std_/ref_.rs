//! The `:ref:` role, which links a label. Named with a trailing underscore,
//! `ref` being a keyword.

use rinx_ast::{InlineNode, InventorySelector};

use crate::explicit_title::split_optional_title;
use crate::inline::regexes::REF_REGEX;
use crate::inline::roles::spec::{ExternalPrefix, RoleSpec};

/// The role this module reads, as the role table lists it.
pub(crate) const SPEC: RoleSpec = RoleSpec {
    names: &["ref"],
    domains: &[],
    external: ExternalPrefix::Accepted,
    doc: "Link a label, showing the title of what it labels.",
};

/// Builds the `InlineNode` for a matched `:ref:` role. Whether the label
/// exists, and the title it shows when no explicit title is written, are
/// decided while rendering.
pub(crate) fn handle_ref_match(m_str: &str) -> InlineNode {
    let caps = REF_REGEX.captures(m_str).unwrap();
    let (display, target) = split_optional_title(&caps["target"]);
    InlineNode::Reference {
        display,
        target,
        span: None,
        inventory: InventorySelector::Any,
    }
}
