//! The `:download:` role, which links a file the site serves for download.
//!
//! Flat under `roles/` for the reason `doc.rs` is: it belongs to the `std`
//! domain alone and takes no `default_domain`. It names a file rather than
//! anything the project index knows, so nothing about it is resolved against
//! the index: whether the file was declared is checked by the site's
//! validation action, and where it is served from is computed while
//! rendering. This handler only reads the markup.

use rinx_ast::{AssetUri, InlineNode};

use crate::explicit_title::split_optional_title;
use crate::inline::regexes::DOWNLOAD_ROLE_REGEX;

/// Builds the `InlineNode` for a matched `:download:` (or `:std:download:`)
/// role.
///
/// A leading `!` turns the role into text that is neither linked nor copied,
/// and — as Sphinx's `XRefRole` does it — that text is everything after the
/// `!`. The target is otherwise kept as written, relative or `/`-absolute,
/// because resolving it needs the referencing document's path.
pub(crate) fn handle_download_match(m_str: &str) -> InlineNode {
    let caps = DOWNLOAD_ROLE_REGEX.captures(m_str).unwrap();
    let content = &caps["target"];
    if let Some(text) = content.strip_prefix('!') {
        return InlineNode::DownloadReference {
            display: None,
            target: AssetUri::new(text),
            link: false,
            span: None,
        };
    }
    let (display, target) = split_optional_title(content);
    InlineNode::DownloadReference {
        display,
        target: AssetUri::new(&target),
        link: true,
        span: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn download(display: Option<&str>, target: &str, link: bool) -> InlineNode {
        InlineNode::DownloadReference {
            display: display.map(str::to_string),
            target: AssetUri::new(target),
            link,
            span: None,
        }
    }

    #[test]
    fn test_handle_download_match_leaves_a_bare_target_untitled() {
        // Given / When
        let node = handle_download_match(":download:`data/sample.csv`");

        // Then
        assert_eq!(node, download(None, "data/sample.csv", true));
    }

    #[test]
    fn test_handle_download_match_splits_an_explicit_title() {
        // Given / When
        let node = handle_download_match(":download:`the data <../data/sample.csv>`");

        // Then
        assert_eq!(node, download(Some("the data"), "../data/sample.csv", true));
    }

    #[test]
    fn test_handle_download_match_keeps_a_source_root_path_as_written() {
        // Given / When
        let node = handle_download_match(":download:`/examples/data/sample.csv`");

        // Then
        assert_eq!(node, download(None, "/examples/data/sample.csv", true));
    }

    #[test]
    fn test_handle_download_match_reads_a_url_as_external() {
        // Given / When
        let node = handle_download_match(":download:`https://example.com/tool.zip`");

        // Then
        assert!(matches!(
            node,
            InlineNode::DownloadReference {
                target: AssetUri::External(_),
                link: true,
                ..
            }
        ));
    }

    #[test]
    fn test_handle_download_match_turns_a_bang_prefix_into_unlinked_text() {
        // Given / When
        let bare = handle_download_match(":download:`!sample.csv`");
        let titled = handle_download_match(":download:`!Data <sample.csv>`");

        // Then — the whole text after the `!`, as for `:doc:`
        assert_eq!(bare, download(None, "sample.csv", false));
        assert_eq!(titled, download(None, "Data <sample.csv>", false));
    }

    #[test]
    fn test_handle_download_match_accepts_the_std_domain_spelling() {
        // Given / When
        let node = handle_download_match(":std:download:`sample.csv`");

        // Then
        assert_eq!(node, download(None, "sample.csv", true));
    }

    /// The single inline node `input` parses to, through the whole pipeline.
    fn only_inline(input: &str) -> InlineNode {
        let doc = crate::parse("test.rst", input);
        let rinx_ast::Node::Paragraph(inlines) = &doc.nodes[0] else {
            panic!("expected a paragraph, got {:?}", doc.nodes[0]);
        };
        assert_eq!(inlines.len(), 1, "{inlines:?}");
        inlines[0].clone()
    }

    #[test]
    fn test_parse_places_a_download_role_at_its_source_position() {
        // Given / When
        let doc = crate::parse("test.rst", "Get :download:`a.py` here.");

        // Then
        let rinx_ast::Node::Paragraph(inlines) = &doc.nodes[0] else {
            panic!("expected a paragraph, got {:?}", doc.nodes[0]);
        };
        assert_eq!(
            inlines[1].span(),
            Some(rinx_ast::Span::new(
                rinx_ast::Position::new(1, 5),
                rinx_ast::Position::new(1, 21)
            ))
        );
    }

    #[test]
    fn test_parse_unescapes_a_download_role_target() {
        // Given — an escaped `!` is text, not the suppression prefix
        // When
        let node = only_inline(r":download:`\!odd.txt`");

        // Then
        assert_eq!(node.with_span(None), download(None, "!odd.txt", true));
    }

    #[test]
    fn test_parse_prefers_the_download_role_over_a_schema_role_of_that_name() {
        // Given — a schema that declares a `download` entity role
        let schema = rinx_entity::load_schema(
            r#"
            [[entity_type]]
            name = "download"

            [[role]]
            name = "download"
            "#,
            &rinx_entity::NoReservedNames,
        )
        .unwrap();
        let ctx = crate::context::ParseCtx::with_domain(rinx_ast::Domain::Py).with_schema(&schema);

        // When
        let parsed = crate::parse_with_ctx("test.rst", ":download:`a.py`", &ctx);

        // Then
        let rinx_ast::Node::Paragraph(inlines) = &parsed.nodes[0] else {
            panic!("expected a paragraph, got {:?}", parsed.nodes[0]);
        };
        assert!(matches!(inlines[0], InlineNode::DownloadReference { .. }));
    }
}
