//! The `.. grid::` and `.. grid-item::` nodes themselves.
//!
//! Note what a grid's children are: `Vec<Node>`, not `Vec<GridItem>`. A grid
//! whose body holds something other than a grid-item is a *warning* in
//! sphinx-design, not an error — the stray content is still parsed and still
//! rendered into the row. Typing the field as a list of items would look
//! tidier and would silently drop that content, which is precisely the failure
//! this directive exists to stop: an unparsed container swallowing everything
//! written inside it. A `.. grid-item::` written outside a grid is likewise a
//! node of its own, warned about and rendered.

use serde::{Deserialize, Serialize};

use crate::node::Node;
use crate::spacing::Spacing;
use crate::span::Span;

use super::child_layout::{ChildAlign, ChildDirection};
use super::column_spec::ColumnSpec;
use super::gutter::Gutter;

/// A responsive row of items — sphinx-design's `.. grid::`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grid {
    /// The directive argument: how many of twelve columns the row splits into
    /// at each breakpoint. `None` when no argument was written, which is
    /// legal — the row then carries no column classes at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<ColumnSpec>,
    /// `:gutter:` — the space kept between items.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gutter: Option<Gutter>,
    /// `:margin:` — space around the container. `None` renders
    /// [`Grid::DEFAULT_MARGIN_CLASSES`], not nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin: Option<Spacing>,
    /// `:padding:` — space inside the container.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding: Option<Spacing>,
    /// `:outline:` — draw a border around the container.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub outline: bool,
    /// `:reverse:` — lay the items out right to left.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reverse: bool,
    /// `:class-container:` — extra classes for the outer `<div>`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub class_container: Vec<String>,
    /// `:class-row:` — extra classes for the row `<div>`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub class_row: Vec<String>,
    /// The directive's content, parsed as ordinary block content — normally a
    /// run of [`Node::Directive`]`(`[`crate::Directive::GridItem`]`)`.
    pub body: Vec<Node>,
    /// Where the directive was written, so a later phase has a position to
    /// report against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

impl Grid {
    /// The classes an *omitted* `:margin:` produces.
    ///
    /// sphinx-design defaults a grid container's margin to `sd-mb-4` — a step
    /// wider than the dropdown's `sd-mb-3`, which is why each directive keeps
    /// its own default rather than the shared [`Spacing`] holding one.
    pub const DEFAULT_MARGIN_CLASSES: &'static [&'static str] = &["sd-mb-4"];

    /// A grid with no argument, no options and no content.
    #[must_use]
    pub fn new() -> Self {
        Self {
            columns: None,
            gutter: None,
            margin: None,
            padding: None,
            outline: false,
            reverse: false,
            class_container: Vec::new(),
            class_row: Vec::new(),
            body: Vec::new(),
            span: None,
        }
    }
}

impl Default for Grid {
    fn default() -> Self {
        Self::new()
    }
}

/// One cell of a grid — sphinx-design's `.. grid-item::`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridItem {
    /// `:columns:` — how many of twelve columns this item spans.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<ColumnSpec>,
    /// `:margin:` — space around the item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin: Option<Spacing>,
    /// `:padding:` — space inside the item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding: Option<Spacing>,
    /// `:child-direction:` — not optional: an omitted value means
    /// [`ChildDirection::Column`], which still emits a class.
    #[serde(default)]
    pub child_direction: ChildDirection,
    /// `:child-align:` — where the content sits along the major axis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_align: Option<ChildAlign>,
    /// `:outline:` — draw a border around the item.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub outline: bool,
    /// `:class:` — extra classes for the item `<div>`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub classes: Vec<String>,
    /// The directive's content, parsed as ordinary block content.
    pub body: Vec<Node>,
    /// Where the directive was written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

impl GridItem {
    /// An item with no options and no content.
    #[must_use]
    pub fn new() -> Self {
        Self {
            columns: None,
            margin: None,
            padding: None,
            child_direction: ChildDirection::default(),
            child_align: None,
            outline: false,
            classes: Vec::new(),
            body: Vec::new(),
            span: None,
        }
    }
}

impl Default for GridItem {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directive::Directive;
    use crate::inline_node::InlineNode;

    /// A one-paragraph body, so a test can tell "content kept" from "content
    /// dropped" without building a whole document.
    fn paragraph(text: &str) -> Node {
        Node::Paragraph(vec![InlineNode::Text(text.to_string())])
    }

    #[test]
    fn test_new_leaves_every_option_unset() {
        // Given / When
        let grid = Grid::new();

        // Then
        assert_eq!(grid.columns, None);
        assert_eq!(grid.gutter, None);
        assert!(!grid.outline);
        assert!(!grid.reverse);
        assert!(grid.body.is_empty());
    }

    #[test]
    fn test_default_matches_new() {
        // Given / When / Then
        assert_eq!(Grid::default(), Grid::new());
        assert_eq!(GridItem::default(), GridItem::new());
    }

    #[test]
    fn test_default_margin_classes_are_a_step_wider_than_a_dropdown_s() {
        // Given / When / Then — sphinx-design's own `sd-mb-4`
        assert_eq!(Grid::DEFAULT_MARGIN_CLASSES, &["sd-mb-4"]);
    }

    #[test]
    fn test_item_defaults_its_direction_rather_than_leaving_it_unset() {
        // Given / When
        let item = GridItem::new();

        // Then
        assert_eq!(item.child_direction, ChildDirection::Column);
        assert_eq!(item.child_align, None);
    }

    #[test]
    fn test_a_grid_body_holds_arbitrary_nodes_not_only_items() {
        // Given — content sphinx-design warns about but still renders
        let mut grid = Grid::new();
        grid.body = vec![
            paragraph("stray"),
            Node::Directive(Directive::GridItem(Box::default())),
        ];

        // When / Then — both survive, which is the whole point of the type
        assert_eq!(grid.body.len(), 2);
    }

    #[test]
    fn test_round_trips_through_json() {
        // Given
        let mut grid = Grid::new();
        grid.columns = Some(ColumnSpec::parse("1 1 2 2").expect("a valid column spec"));
        grid.gutter = Some(Gutter::parse("3").expect("a valid gutter"));
        grid.outline = true;
        let mut item = GridItem::new();
        item.columns = Some(ColumnSpec::parse("auto").expect("a valid column spec"));
        item.child_align = Some(ChildAlign::Center);
        item.body = vec![paragraph("cell")];
        grid.body = vec![Node::Directive(Directive::GridItem(Box::new(item)))];

        // When
        let json = serde_json::to_string(&grid).expect("serializable");
        let restored: Grid = serde_json::from_str(&json).expect("deserializable");

        // Then
        assert_eq!(restored, grid);
    }

    #[test]
    fn test_unset_options_stay_out_of_the_serialized_form() {
        // Given — a bare `.. grid::` with one empty item
        let grid = Grid::new();

        // When
        let json = serde_json::to_string(&grid).expect("serializable");

        // Then — only the body survives, so an `.ast` entry stays small
        assert_eq!(json, r#"{"body":[]}"#);
    }
}
