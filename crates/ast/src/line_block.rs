use serde::{Deserialize, Serialize};

use crate::inline_node::InlineNode;

/// One entry of a [`crate::Node::LineBlock`]: either a rendered line, or a
/// further-indented run of lines nested one level deeper — docutils' own
/// `nest_line_block_segment` grouping, computed once at parse time so the
/// renderer only has to walk the tree it's given.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineBlockItem {
    /// One line of content. Empty when the source line was a bare `|` — a
    /// deliberately blank line in the rendered output, as opposed to a truly
    /// blank source line, which ends the block instead.
    Line(Vec<InlineNode>),
    /// A run of lines indented further than their surrounding siblings,
    /// nested one level deeper.
    Nested(Vec<LineBlockItem>),
}
