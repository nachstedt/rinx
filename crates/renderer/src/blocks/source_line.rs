//! The source line each block was written on, for a preview to scroll by.
//!
//! The language server's preview passes the document's [`SourceMap`], and
//! each block the parser placed then carries `data-source-line` on the first
//! tag it writes — the 0-based line, as an editor counts it — and a literal or
//! doctest block `data-source-end-line` as well, so a reader can interpolate
//! through its lines. A block written in another file (an `.. include::`d
//! fragment, `rst_prolog`) is left unmarked: its lines are not the document's.
//!
//! The build passes no map — a [`SourceMap`] has no serde impl, so a build
//! phase holds none — and its pages carry no attribute.

use std::fmt::Write as _;

use rinx_ast::{BlockId, SourceMap};

/// Which of a block's lines its first tag names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LineExtent {
    /// The line the block starts on.
    Start,
    /// Its first and its last line, for a block whose lines show one to one.
    StartAndEnd,
}

/// Marks the first start tag written in `html` from byte `from` with the
/// lines `map` records for `block`; nothing without a map, without lines,
/// for lines in another file, or when no tag was written.
pub(super) fn mark_source_lines(
    html: &mut String,
    from: usize,
    block: Option<BlockId>,
    extent: LineExtent,
    map: Option<&SourceMap>,
) {
    let Some(lines) = map.zip(block).and_then(|(map, id)| map.block_lines(id)) else {
        return;
    };
    if lines.file.is_some() {
        return;
    }
    let mut attributes = format!(
        " data-source-line=\"{}\"",
        lines.start.line.saturating_sub(1)
    );
    if extent == LineExtent::StartAndEnd {
        let _ = write!(
            attributes,
            " data-source-end-line=\"{}\"",
            lines.end.line.saturating_sub(1)
        );
    }
    mark_first_tag(html, from, &attributes);
}

/// Inserts `attributes` after the name of the first start tag in `html` from
/// byte `from` — unless a block nested in it marked that tag already, which a
/// container writing nothing but its body leaves to the body.
fn mark_first_tag(html: &mut String, from: usize, attributes: &str) {
    let bytes = html.as_bytes();
    let Some(open) = (from..bytes.len().saturating_sub(1))
        .find(|&at| bytes[at] == b'<' && bytes[at + 1].is_ascii_alphabetic())
    else {
        return;
    };
    let name_end = bytes[open + 1..]
        .iter()
        .position(|byte| !byte.is_ascii_alphanumeric())
        .map_or(bytes.len(), |length| open + 1 + length);
    if html[name_end..].starts_with(" data-source-line=") {
        return;
    }
    html.insert_str(name_end, attributes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{FileId, Position, Span};

    /// A map recording `lines` as its only block.
    fn map_of(lines: Span) -> (SourceMap, BlockId) {
        let mut map = SourceMap::default();
        let id = map.record_block_lines(lines);
        (map, id)
    }

    fn span(first: u32, last: u32) -> Span {
        Span::new(Position::new(first, 1), Position::new(last, 5))
    }

    #[test]
    fn test_mark_first_tag_inserts_after_the_tag_name() {
        // Given
        let mut html = "<p class=\"x\">Text</p>".to_string();

        // When
        mark_first_tag(&mut html, 0, " data-source-line=\"3\"");

        // Then
        assert_eq!(html, "<p data-source-line=\"3\" class=\"x\">Text</p>");
    }

    #[test]
    fn test_mark_first_tag_skips_what_was_written_before_from_and_closing_tags() {
        // Given — a closing tag and text lie between `from` and the block's tag
        let mut html = "<p>Before</p>\n</div> text <hr />".to_string();

        // When
        mark_first_tag(&mut html, 13, " a=\"1\"");

        // Then
        assert_eq!(html, "<p>Before</p>\n</div> text <hr a=\"1\" />");
    }

    #[test]
    fn test_mark_first_tag_leaves_a_tag_a_nested_block_marked() {
        // Given
        let mut html = "<p data-source-line=\"4\">Body</p>".to_string();

        // When
        mark_first_tag(&mut html, 0, " data-source-line=\"2\"");

        // Then
        assert_eq!(html, "<p data-source-line=\"4\">Body</p>");
    }

    #[test]
    fn test_mark_first_tag_leaves_html_without_a_start_tag_alone() {
        // Given
        let mut html = "<p>Before</p>text &lt;b&gt;<".to_string();

        // When
        mark_first_tag(&mut html, 12, " a=\"1\"");

        // Then
        assert_eq!(html, "<p>Before</p>text &lt;b&gt;<");
    }

    #[test]
    fn test_mark_source_lines_names_the_zero_based_start_line() {
        // Given
        let (map, id) = map_of(span(3, 4));
        let mut html = "<p>Text</p>".to_string();

        // When
        mark_source_lines(&mut html, 0, Some(id), LineExtent::Start, Some(&map));

        // Then
        assert_eq!(html, "<p data-source-line=\"2\">Text</p>");
    }

    #[test]
    fn test_mark_source_lines_names_the_end_line_when_asked() {
        // Given
        let (map, id) = map_of(span(3, 7));
        let mut html = "<pre>code</pre>".to_string();

        // When
        mark_source_lines(&mut html, 0, Some(id), LineExtent::StartAndEnd, Some(&map));

        // Then
        assert_eq!(
            html,
            "<pre data-source-line=\"2\" data-source-end-line=\"6\">code</pre>"
        );
    }

    #[test]
    fn test_mark_source_lines_marks_nothing_without_a_map_or_a_block() {
        // Given
        let (map, id) = map_of(span(3, 4));
        let mut without_map = "<p>Text</p>".to_string();
        let mut without_block = "<p>Text</p>".to_string();

        // When
        mark_source_lines(&mut without_map, 0, Some(id), LineExtent::Start, None);
        mark_source_lines(&mut without_block, 0, None, LineExtent::Start, Some(&map));

        // Then
        assert_eq!(without_map, "<p>Text</p>");
        assert_eq!(without_block, "<p>Text</p>");
    }

    #[test]
    fn test_mark_source_lines_marks_nothing_for_lines_in_another_file() {
        // Given — a block an `.. include::` spliced in
        let mut lines = span(1, 1);
        lines.file = Some(FileId::new(0));
        let (map, id) = map_of(lines);
        let mut html = "<p>Included</p>".to_string();

        // When
        mark_source_lines(&mut html, 0, Some(id), LineExtent::Start, Some(&map));

        // Then
        assert_eq!(html, "<p>Included</p>");
    }

    /// The body of `text`'s page, its lines named when `marked`.
    fn page_of(text: &str, marked: bool) -> String {
        let parsed = rinx_parser::parse("doc.rst", text, &rinx_parser::ParseCtx::default());
        let embedded = crate::EmbeddedAssets::new();
        let mut assets = crate::PageAssets::embedding(&embedded);
        assets.source_lines = marked.then_some(&parsed.source_map);
        crate::render_with_page_assets(
            &parsed.document,
            &rinx_index::ProjectIndex::default(),
            "doc",
            &crate::config::SiteConfig::default(),
            assets,
            rinx_entity::EntitySchema::empty_ref(),
            &crate::blocks::EntityTemplates::new(),
        )
        .html
    }

    const DOCUMENT: &str = "\
Title
=====

A paragraph.

- An item.

term
   Its definition.

-v  Talk more.

.. note::

   Noted.

::

   code
   more
";

    #[test]
    fn test_a_page_names_where_each_block_and_item_was_written() {
        // When
        let html = page_of(DOCUMENT, true);

        // Then
        for marked in [
            "<h1 data-source-line=\"0\"",
            "<p data-source-line=\"3\">A paragraph.",
            "<li data-source-line=\"5\">",
            "<dt data-source-line=\"7\">term",
            "<dt data-source-line=\"10\"><kbd>",
            "<p data-source-line=\"14\">Noted.",
            "data-source-line=\"16\" data-source-end-line=\"19\"",
        ] {
            assert!(html.contains(marked), "no `{marked}` in {html}");
        }
        assert!(
            html.contains("<ul>\n"),
            "a list names no line of its own: {html}"
        );
    }

    #[test]
    fn test_a_directive_names_its_marker_line() {
        // When
        let html = page_of(DOCUMENT, true);

        // Then
        let note = html
            .find("data-source-line=\"12\"")
            .expect("the note's marker line");
        assert!(html[note..].contains("Noted."), "{html}");
    }

    #[test]
    fn test_a_page_without_a_map_names_no_line() {
        // When
        let html = page_of(DOCUMENT, false);

        // Then
        assert!(!html.contains("data-source"), "{html}");
    }
}
