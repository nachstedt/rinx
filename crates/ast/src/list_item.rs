use serde::{Deserialize, Serialize};

use crate::block_id::BlockId;
use crate::node::Node;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListItem {
    pub nodes: Vec<Node>,
    /// The item's lines, bullet or enumerator included, as the parse's
    /// source map records them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<BlockId>,
}
