//! The `:program:` role, which shows the name of a program.

use rinx_ast::InlineNode;

use crate::inline::regexes::PROGRAM_ROLE_REGEX;
use crate::inline::roles::spec::{ExternalPrefix, RoleSpec};

/// The role this module reads, as the role table lists it.
pub(crate) const SPEC: RoleSpec = RoleSpec {
    names: &["program"],
    domains: &[],
    external: ExternalPrefix::Unread,
    doc: "Show the name of a program.",
};

/// Builds the `InlineNode` for a matched `:program:` role.
pub(crate) fn handle_program_match(m_str: &str) -> InlineNode {
    let caps = PROGRAM_ROLE_REGEX.captures(m_str).unwrap();
    InlineNode::Program(caps["name"].to_string())
}
