//! `.. grid::` / `.. grid-item::` rendering — sphinx-design's responsive row.
//!
//! The elements this writes are what `GridDirective` and `GridItemDirective`
//! build, class for class, so a site already styled for sphinx-design keeps
//! its appearance:
//!
//! ```html
//! <div class="sd-container-fluid sd-sphinx-override sd-mb-4 docutils">
//!   <div class="sd-row sd-row-cols-2 sd-row-cols-xs-2 … sd-g-3 … docutils">
//!     <div class="sd-col sd-d-flex-column sd-col-6 … docutils">…</div>
//!   </div>
//! </div>
//! ```
//!
//! Three details are easy to get wrong and are therefore spelled out here.
//! The trailing `docutils` on every `<div>` is sphinx-design's own
//! `visit_container`, which replaces docutils' `docutils container` with a
//! bare `docutils` for its components — dropping it would leave a page's own
//! stylesheet without the selector it may be written against. A grid is *two*
//! nested `<div>`s, not one: the outer carries the margin and padding, the
//! inner the columns and the gutter, and collapsing them would make
//! `:margin:` and `:gutter:` fight over the same box. And a grid-item always
//! carries a `sd-d-flex-*` class, since an omitted `:child-direction:` means
//! `column` rather than nothing.

use std::fmt::Write as _;

use rusty_sphinx_ast::{ColumnPrefix, Grid, GridItem, SpacingKind};

use crate::RenderCtx;

use super::dispatch::render_nodes;

/// Renders a `.. grid::` directive.
pub(super) fn render_grid(html: &mut String, grid: &Grid, ctx: &mut RenderCtx<'_>) {
    let _ = writeln!(
        html,
        "<div class=\"{}\">",
        class_attribute(&container_classes(grid))
    );
    let _ = writeln!(
        html,
        "<div class=\"{}\">",
        class_attribute(&row_classes(grid))
    );
    render_nodes(html, &grid.body, ctx);
    let _ = writeln!(html, "</div>");
    let _ = writeln!(html, "</div>");
}

/// Renders a `.. grid-item::` directive.
pub(super) fn render_grid_item(html: &mut String, item: &GridItem, ctx: &mut RenderCtx<'_>) {
    let _ = writeln!(
        html,
        "<div class=\"{}\">",
        class_attribute(&item_classes(item))
    );
    render_nodes(html, &item.body, ctx);
    let _ = writeln!(html, "</div>");
}

/// The outer `<div>`'s classes: the fixed two, the margin, the padding, the
/// outline and the author's own — in sphinx-design's order.
fn container_classes(grid: &Grid) -> Vec<String> {
    let mut classes = vec![
        "sd-container-fluid".to_string(),
        "sd-sphinx-override".to_string(),
    ];
    match grid.margin {
        // An omitted `:margin:` is not "no margin": sphinx-design defaults the
        // container to a bottom margin so consecutive grids do not touch.
        None => classes.extend(
            Grid::DEFAULT_MARGIN_CLASSES
                .iter()
                .map(|&class| class.to_string()),
        ),
        Some(margin) => classes.extend(margin.css_classes(SpacingKind::Margin)),
    }
    if let Some(padding) = grid.padding {
        classes.extend(padding.css_classes(SpacingKind::Padding));
    }
    if grid.outline {
        classes.push(BORDER_CLASS.to_string());
    }
    classes.extend(grid.class_container.iter().cloned());
    push_docutils(&mut classes);
    classes
}

/// The row `<div>`'s classes: the column counts, the gutter, the reversal and
/// the author's own.
fn row_classes(grid: &Grid) -> Vec<String> {
    let mut classes = vec!["sd-row".to_string()];
    if let Some(columns) = grid.columns {
        classes.extend(columns.css_classes(ColumnPrefix::Row));
    }
    if let Some(gutter) = grid.gutter {
        classes.extend(gutter.css_classes());
    }
    if grid.reverse {
        classes.push("sd-flex-row-reverse".to_string());
    }
    classes.extend(grid.class_row.iter().cloned());
    push_docutils(&mut classes);
    classes
}

/// The item `<div>`'s classes: the cell itself, its flex direction, its span,
/// its spacing, its alignment, its outline and the author's own.
fn item_classes(item: &GridItem) -> Vec<String> {
    let mut classes = vec!["sd-col".to_string(), item.child_direction.css_class()];
    if let Some(columns) = item.columns {
        classes.extend(columns.css_classes(ColumnPrefix::Item));
    }
    if let Some(margin) = item.margin {
        classes.extend(margin.css_classes(SpacingKind::Margin));
    }
    if let Some(padding) = item.padding {
        classes.extend(padding.css_classes(SpacingKind::Padding));
    }
    if let Some(align) = item.child_align {
        classes.push(align.css_class());
    }
    if item.outline {
        classes.push(BORDER_CLASS.to_string());
    }
    classes.extend(item.classes.iter().cloned());
    push_docutils(&mut classes);
    classes
}

/// The class an `:outline:` flag adds, on a grid and on an item alike.
const BORDER_CLASS: &str = "sd-border-1";

/// Appends the class sphinx-design's own container visitor writes last.
fn push_docutils(classes: &mut Vec<String>) {
    classes.push("docutils".to_string());
}

/// Joins classes into an escaped `class` attribute value.
fn class_attribute(classes: &[String]) -> String {
    html_escape::encode_double_quoted_attribute(&classes.join(" ")).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::render_test_support::render_directive_html;
    use rusty_sphinx_ast::{
        ChildAlign, ChildDirection, ColumnSpec, Directive, Gutter, InlineNode, Node, Spacing,
        SpacingValue,
    };
    use rusty_sphinx_index::ProjectIndex;

    fn grid_html(grid: Grid) -> String {
        render_directive_html(
            &Directive::Grid(Box::new(grid)),
            &ProjectIndex::default(),
            "index",
        )
    }

    fn item_html(item: GridItem) -> String {
        render_directive_html(
            &Directive::GridItem(Box::new(item)),
            &ProjectIndex::default(),
            "index",
        )
    }

    /// A one-paragraph body, so a test can tell "content rendered" from
    /// "content dropped".
    fn paragraph(text: &str) -> Node {
        Node::Paragraph(vec![InlineNode::Text(text.to_string())])
    }

    /// The class list of the *n*-th `<div>` the renderer wrote, so a test can
    /// assert about one box without matching the whole document.
    fn div_classes(html: &str, index: usize) -> &str {
        html.match_indices("<div class=\"")
            .nth(index)
            .map(|(start, marker)| {
                let rest = &html[start + marker.len()..];
                &rest[..rest.find('"').expect("a closed attribute")]
            })
            .expect("that many divs")
    }

    #[test]
    fn test_container_classes_default_the_margin_when_none_was_written() {
        // Given
        let grid = Grid::new();

        // When
        let classes = container_classes(&grid);

        // Then — sphinx-design's own `sd-mb-4`, not nothing
        assert_eq!(
            classes,
            vec![
                "sd-container-fluid",
                "sd-sphinx-override",
                "sd-mb-4",
                "docutils",
            ]
        );
    }

    #[test]
    fn test_container_classes_replace_the_default_margin_when_one_was_written() {
        // Given
        let grid = Grid {
            margin: Some(Spacing::All(SpacingValue::Two)),
            ..Grid::new()
        };

        // When
        let classes = container_classes(&grid);

        // Then
        assert!(classes.contains(&"sd-m-2".to_string()));
        assert!(!classes.contains(&"sd-mb-4".to_string()));
    }

    #[test]
    fn test_container_classes_carry_the_padding_and_the_outline() {
        // Given
        let grid = Grid {
            padding: Some(Spacing::All(SpacingValue::Three)),
            outline: true,
            class_container: vec!["mine".to_string()],
            ..Grid::new()
        };

        // When
        let classes = container_classes(&grid);

        // Then — in sphinx-design's order: padding, outline, author's own
        assert_eq!(
            classes,
            vec![
                "sd-container-fluid",
                "sd-sphinx-override",
                "sd-mb-4",
                "sd-p-3",
                "sd-border-1",
                "mine",
                "docutils",
            ]
        );
    }

    #[test]
    fn test_row_classes_spell_out_every_breakpoint() {
        // Given — the corpus' own `.. grid:: 1 1 2 2`
        let grid = Grid {
            columns: Some(ColumnSpec::parse("1 1 2 2").expect("a valid column spec")),
            ..Grid::new()
        };

        // When
        let classes = row_classes(&grid);

        // Then
        assert_eq!(
            classes,
            vec![
                "sd-row",
                "sd-row-cols-1",
                "sd-row-cols-xs-1",
                "sd-row-cols-sm-1",
                "sd-row-cols-md-2",
                "sd-row-cols-lg-2",
                "docutils",
            ]
        );
    }

    #[test]
    fn test_row_classes_carry_the_gutter_and_the_reversal() {
        // Given
        let grid = Grid {
            gutter: Some(Gutter::parse("3").expect("a valid gutter")),
            reverse: true,
            class_row: vec!["mine".to_string()],
            ..Grid::new()
        };

        // When
        let classes = row_classes(&grid);

        // Then
        assert_eq!(
            classes,
            vec![
                "sd-row",
                "sd-g-3",
                "sd-g-xs-3",
                "sd-g-sm-3",
                "sd-g-md-3",
                "sd-g-lg-3",
                "sd-flex-row-reverse",
                "mine",
                "docutils",
            ]
        );
    }

    #[test]
    fn test_row_classes_omit_the_column_family_when_no_argument_was_written() {
        // Given — a bare `.. grid::`, which is legal
        let grid = Grid::new();

        // When
        let classes = row_classes(&grid);

        // Then
        assert_eq!(classes, vec!["sd-row", "docutils"]);
    }

    #[test]
    fn test_item_classes_always_carry_a_flex_direction() {
        // Given — an item with no options at all
        let item = GridItem::new();

        // When
        let classes = item_classes(&item);

        // Then — an omitted `:child-direction:` is `column`, not nothing
        assert_eq!(classes, vec!["sd-col", "sd-d-flex-column", "docutils"]);
    }

    #[test]
    fn test_item_classes_cover_every_option() {
        // Given
        let item = GridItem {
            columns: Some(ColumnSpec::parse("12 12 8 8").expect("a valid column spec")),
            margin: Some(Spacing::All(SpacingValue::One)),
            padding: Some(Spacing::All(SpacingValue::Two)),
            child_direction: ChildDirection::Row,
            child_align: Some(ChildAlign::Center),
            outline: true,
            classes: vec!["mine".to_string()],
            ..GridItem::new()
        };

        // When
        let classes = item_classes(&item);

        // Then
        assert_eq!(
            classes,
            vec![
                "sd-col",
                "sd-d-flex-row",
                "sd-col-12",
                "sd-col-xs-12",
                "sd-col-sm-12",
                "sd-col-md-8",
                "sd-col-lg-8",
                "sd-m-1",
                "sd-p-2",
                "sd-align-major-center",
                "sd-border-1",
                "mine",
                "docutils",
            ]
        );
    }

    #[test]
    fn test_class_attribute_escapes_an_author_written_class() {
        // Given
        let classes = vec!["a\"b".to_string()];

        // When
        let attribute = class_attribute(&classes);

        // Then
        assert_eq!(attribute, "a&quot;b");
    }

    #[test]
    fn test_render_nests_the_row_inside_the_container() {
        // Given
        let grid = Grid {
            columns: Some(ColumnSpec::parse("2").expect("a valid column spec")),
            body: vec![Node::Directive(Directive::GridItem(Box::new(GridItem {
                body: vec![paragraph("Hello")],
                ..GridItem::new()
            })))],
            ..Grid::new()
        };

        // When
        let html = grid_html(grid);

        // Then — container, row, item, in that order, each closed
        assert!(div_classes(&html, 0).starts_with("sd-container-fluid"));
        assert!(div_classes(&html, 1).starts_with("sd-row"));
        assert!(div_classes(&html, 2).starts_with("sd-col"));
        assert_eq!(html.matches("</div>").count(), 3);
        assert!(html.contains("Hello"));
    }

    #[test]
    fn test_render_keeps_content_a_grid_was_not_supposed_to_hold() {
        // Given — prose written straight into a grid, which sphinx-design
        // warns about and still renders
        let grid = Grid {
            body: vec![paragraph("Stray prose")],
            ..Grid::new()
        };

        // When
        let html = grid_html(grid);

        // Then
        assert!(html.contains("Stray prose"), "got: {html}");
    }

    #[test]
    fn test_render_draws_an_item_on_its_own() {
        // Given — a grid-item written outside any grid, warned about while
        // parsing but still a box of its own
        let item = GridItem {
            body: vec![paragraph("Orphan")],
            ..GridItem::new()
        };

        // When
        let html = item_html(item);

        // Then
        assert!(div_classes(&html, 0).starts_with("sd-col"));
        assert!(html.contains("Orphan"));
    }
}
