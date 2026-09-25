//! The presentation shell shared by every *option-bearing* table directive —
//! `.. list-table::`/`.. csv-table::` (see [`super::data_table`]) and
//! `.. table::` (see [`super::table_directive`]): the `<table ...>` open tag,
//! `<caption>`, the `:name:` anchor, and the `:widths:`-derived `<colgroup>`.
//! None of this applies to a bare grid/simple `Node::Table`, which has no
//! such options.

use rinx_ast::{TableAlign, TableWidths, TargetName};
use std::fmt::Write as _;

/// Emits a `:name:` anchor exactly like an explicit hyperlink target
/// (`Node::Target` with no `uri`, see `render_nodes`) — reusing that same
/// mechanism rather than inventing a new target-location concept.
pub(super) fn render_table_name_anchor(html: &mut String, name: Option<&TargetName>) {
    if let Some(target_name) = name {
        let escaped = html_escape::encode_text(target_name.as_str());
        let _ = writeln!(html, "<a id=\"{escaped}\"></a>");
    }
}

/// Opens a `<table>` tag. `classes` is written verbatim as the leading
/// `class` attribute names — `data_table` puts its directive's own name
/// first there, `table_directive` puts nothing, since a bare grid/simple
/// table has no name of its own to lend it — with an `align-*` class
/// appended for `:align:`. The `class` attribute is omitted entirely when
/// the resulting list is empty, matching a bare grid/simple table's plain
/// `<table>`.
pub(super) fn render_table_open_tag(
    html: &mut String,
    classes: &[String],
    align: Option<TableAlign>,
    width: Option<&str>,
) {
    let mut class_list = classes.to_vec();
    if let Some(align) = align {
        class_list.push(format!("align-{}", align.as_str()));
    }
    let _ = write!(html, "<table");
    if !class_list.is_empty() {
        let class_string = class_list.join(" ");
        let class_attr = html_escape::encode_double_quoted_attribute(&class_string);
        let _ = write!(html, " class=\"{class_attr}\"");
    }
    if let Some(width) = width {
        let width_escaped = html_escape::encode_double_quoted_attribute(width);
        let _ = write!(html, " style=\"width: {width_escaped}\"");
    }
    let _ = writeln!(html, ">");
}

/// Emits a `<caption>` for the directive's title argument, when given.
pub(super) fn render_table_caption(html: &mut String, title: Option<&str>) {
    if let Some(title) = title {
        let title_escaped = html_escape::encode_text(title);
        let _ = writeln!(html, "<caption>{title_escaped}</caption>");
    }
}

/// Renders a `<colgroup>` for `:widths:`'s explicit-integer-list form,
/// normalizing the values as *relative* weights (per the spec) rather than
/// literal percentages. `Auto`/`Grid`/`None` all mean "let the renderer
/// decide" — no `<colgroup>` at all.
pub(super) fn render_table_colgroup(html: &mut String, widths: Option<&TableWidths>) {
    let Some(TableWidths::Explicit(cols)) = widths else {
        return;
    };
    let total: u32 = cols.iter().sum();
    if total == 0 {
        return;
    }
    let _ = writeln!(html, "<colgroup>");
    for col in cols {
        let pct = f64::from(*col) * 100.0 / f64::from(total);
        let _ = writeln!(html, "<col style=\"width: {pct:.2}%\" />");
    }
    let _ = writeln!(html, "</colgroup>");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_table_name_anchor_emits_an_id_anchor() {
        // Given
        let mut html = String::new();
        let name = TargetName::new("fruit-table");

        // When
        render_table_name_anchor(&mut html, Some(&name));

        // Then
        assert_eq!(html, "<a id=\"fruit-table\"></a>\n");
    }

    #[test]
    fn test_render_table_name_anchor_emits_nothing_when_absent() {
        // Given
        let mut html = String::new();

        // When
        render_table_name_anchor(&mut html, None);

        // Then
        assert!(html.is_empty());
    }

    #[test]
    fn test_render_table_open_tag_omits_class_attribute_when_nothing_to_say() {
        // Given
        let mut html = String::new();

        // When
        render_table_open_tag(&mut html, &[], None, None);

        // Then
        assert_eq!(html, "<table>\n");
    }

    #[test]
    fn test_render_table_open_tag_writes_extra_classes_and_align_class() {
        // Given
        let mut html = String::new();
        let classes = vec!["compact".to_string()];

        // When
        render_table_open_tag(&mut html, &classes, Some(TableAlign::Center), None);

        // Then
        assert_eq!(html, "<table class=\"compact align-center\">\n");
    }

    #[test]
    fn test_render_table_open_tag_writes_width_as_inline_style() {
        // Given
        let mut html = String::new();

        // When
        render_table_open_tag(&mut html, &[], None, Some("50%"));

        // Then
        assert_eq!(html, "<table style=\"width: 50%\">\n");
    }

    #[test]
    fn test_render_table_caption_emits_a_caption_element() {
        // Given
        let mut html = String::new();

        // When
        render_table_caption(&mut html, Some("Fruit"));

        // Then
        assert_eq!(html, "<caption>Fruit</caption>\n");
    }

    #[test]
    fn test_render_table_caption_emits_nothing_when_absent() {
        // Given
        let mut html = String::new();

        // When
        render_table_caption(&mut html, None);

        // Then
        assert!(html.is_empty());
    }

    #[test]
    fn test_render_table_colgroup_emits_relative_widths_as_percentages() {
        // Given
        let mut html = String::new();
        let widths = TableWidths::Explicit(vec![30, 70]);

        // When
        render_table_colgroup(&mut html, Some(&widths));

        // Then
        assert!(html.contains("<col style=\"width: 30.00%\" />"));
        assert!(html.contains("<col style=\"width: 70.00%\" />"));
    }

    #[test]
    fn test_render_table_colgroup_skipped_for_auto_grid_and_none() {
        // Given
        let mut html = String::new();

        // When
        render_table_colgroup(&mut html, Some(&TableWidths::Auto));
        render_table_colgroup(&mut html, Some(&TableWidths::Grid));
        render_table_colgroup(&mut html, None);

        // Then
        assert!(html.is_empty());
    }
}
