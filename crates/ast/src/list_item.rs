use serde::{Deserialize, Serialize};

use crate::node::Node;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListItem {
    pub nodes: Vec<Node>,
}
