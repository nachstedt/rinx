//! The `:doc:` role, which links a whole document by its name.
//!
//! Flat under `roles/` for the reason `any.rs` is: it belongs to the `std`
//! domain alone, which has no role family of its own here beyond `:option:`,
//! and it takes no `default_domain`. Whether the document exists — and what
//! its title is — is decided while rendering, against the merged project
//! index; this handler only reads the markup.

use rinx_ast::{InlineNode, InventorySelector};

use crate::explicit_title::split_optional_title;
use crate::inline::regexes::DOC_ROLE_REGEX;

/// Builds the `InlineNode` for a matched `:doc:` (or `:std:doc:`) role.
///
/// A leading `!` turns the reference into text that is never looked up, and
/// — as Sphinx's `XRefRole` does it — that text is everything after the `!`,
/// an angle-bracketed target included: no title is split off something that
/// will not be a link. The target is otherwise kept as written, relative or
/// `/`-absolute, because resolving it needs the referencing document's path,
/// which the renderer has.
pub(crate) fn handle_doc_match(m_str: &str) -> InlineNode {
    let caps = DOC_ROLE_REGEX.captures(m_str).unwrap();
    let content = &caps["target"];
    if let Some(text) = content.strip_prefix('!') {
        return InlineNode::DocReference {
            display: None,
            target: text.to_string(),
            link: false,
            span: None,
            inventory: InventorySelector::Any,
        };
    }
    let (display, target) = split_optional_title(content);
    InlineNode::DocReference {
        display,
        target,
        link: true,
        span: None,
        inventory: InventorySelector::Any,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(display: Option<&str>, target: &str, link: bool) -> InlineNode {
        InlineNode::DocReference {
            display: display.map(str::to_string),
            target: target.to_string(),
            link,
            span: None,
            inventory: InventorySelector::Any,
        }
    }

    #[test]
    fn test_handle_doc_match_leaves_a_bare_target_untitled() {
        // Given / When
        let node = handle_doc_match(":doc:`guide/intro`");

        // Then
        assert_eq!(node, doc(None, "guide/intro", true));
    }

    #[test]
    fn test_handle_doc_match_splits_an_explicit_title() {
        // Given / When
        let node = handle_doc_match(":doc:`the introduction <../intro>`");

        // Then
        assert_eq!(node, doc(Some("the introduction"), "../intro", true));
    }

    #[test]
    fn test_handle_doc_match_turns_a_bang_prefix_into_unlinked_text() {
        // Given / When
        let bare = handle_doc_match(":doc:`!intro`");
        let titled = handle_doc_match(":doc:`!Intro <intro>`");

        // Then — Sphinx 9.1 shows `Intro <intro>` whole for the second
        assert_eq!(bare, doc(None, "intro", false));
        assert_eq!(titled, doc(None, "Intro <intro>", false));
    }

    #[test]
    fn test_handle_doc_match_accepts_the_std_domain_spelling() {
        // Given / When
        let node = handle_doc_match(":std:doc:`/index`");

        // Then
        assert_eq!(node, doc(None, "/index", true));
    }

    #[test]
    fn test_handle_doc_match_keeps_a_tilde_in_the_target() {
        // Given — `~` is markup only to the domain-object roles
        // When
        let node = handle_doc_match(":doc:`~intro`");

        // Then
        assert_eq!(node, doc(None, "~intro", true));
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
    fn test_parse_places_a_doc_role_at_its_source_position() {
        // Given / When
        let doc = crate::parse("test.rst", "See :doc:`intro` here.");

        // Then
        let rinx_ast::Node::Paragraph(inlines) = &doc.nodes[0] else {
            panic!("expected a paragraph, got {:?}", doc.nodes[0]);
        };
        assert_eq!(
            inlines[1].span(),
            Some(rinx_ast::Span::new(
                rinx_ast::Position::new(1, 5),
                rinx_ast::Position::new(1, 17)
            ))
        );
    }

    #[test]
    fn test_parse_reads_an_external_prefix_on_a_doc_role() {
        // Given / When
        let external = only_inline(":external:doc:`tutorial/index`");
        let named = only_inline(":external+python:doc:`tutorial/index`");
        let std_named = only_inline(":external+python:std:doc:`tutorial/index`");

        // Then
        assert!(matches!(
            external,
            InlineNode::DocReference {
                inventory: InventorySelector::ExternalOnly,
                ..
            }
        ));
        for node in [named, std_named] {
            assert!(matches!(
                node,
                InlineNode::DocReference {
                    inventory: InventorySelector::Named(_),
                    ..
                }
            ));
        }
    }

    #[test]
    fn test_parse_unescapes_a_doc_role_target() {
        // Given — an escaped `!` is text, not the suppression prefix
        // When
        let node = only_inline(r":doc:`\!odd`");

        // Then
        assert!(matches!(
            node,
            InlineNode::DocReference { ref target, link: true, .. } if target == "!odd"
        ));
    }

    #[test]
    fn test_parse_prefers_the_doc_role_over_a_schema_role_of_that_name() {
        // Given — a schema that declares a `doc` entity role
        let schema = rinx_entity::load_schema(
            r#"
            [[entity_type]]
            name = "doc"

            [[role]]
            name = "doc"
            "#,
            &rinx_entity::NoReservedNames,
        )
        .unwrap();
        let ctx = crate::context::ParseCtx::with_domain(rinx_ast::Domain::Py).with_schema(&schema);

        // When
        let parsed = crate::parse_with_ctx("test.rst", ":doc:`intro`", &ctx);

        // Then
        let rinx_ast::Node::Paragraph(inlines) = &parsed.nodes[0] else {
            panic!("expected a paragraph, got {:?}", parsed.nodes[0]);
        };
        assert!(matches!(inlines[0], InlineNode::DocReference { .. }));
    }
}
