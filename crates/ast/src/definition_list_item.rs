use serde::{Deserialize, Serialize};

use crate::inline_node::InlineNode;
use crate::node::Node;

/// A single `term` / indented-definition entry in a generic RST definition list.
///
/// Unlike [`GlossaryEntry`](crate::glossary_entry::GlossaryEntry), `term` is
/// `Vec<InlineNode>` (not `String`) so inline markup/roles (e.g. `:mod:`) in
/// the term text render correctly, and there is exactly one term per entry
/// (glossary's "multiple terms share one definition" quirk is
/// glossary-specific and not modeled here).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefinitionListItem {
    pub term: Vec<InlineNode>,
    pub definition: Vec<Node>,
}
