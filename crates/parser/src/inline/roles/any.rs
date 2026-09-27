//! The `:any:` role, which names a target without saying what kind of thing
//! it is.
//!
//! Flat under `roles/` for the reason `math.rs` is: it belongs to no domain —
//! it searches all of them — so its handler takes no `default_domain`. What it
//! resolves to is decided while rendering, against the merged project index;
//! this handler only reads the markup.

use rinx_ast::{InlineNode, InventorySelector};

use crate::explicit_title::split_optional_title;
use crate::inline::regexes::ANY_ROLE_REGEX;

/// Builds the `InlineNode` for a matched `:any:` role.
///
/// A leading `!` is consumed before the explicit title is split, as Sphinx's
/// `XRefRole` does it, and turns the reference into a literal that is never
/// looked up. A `~` gets no such treatment: Sphinx's `:any:` does not shorten
/// (it looks `~pkg.run` up verbatim, and finds nothing), so neither does this.
pub(crate) fn handle_any_match(m_str: &str) -> InlineNode {
    let caps = ANY_ROLE_REGEX.captures(m_str).unwrap();
    let content = &caps["target"];
    let (link, content) = match content.strip_prefix('!') {
        Some(rest) => (false, rest),
        None => (true, content),
    };
    let (display, target) = split_optional_title(content);
    InlineNode::AnyReference {
        display,
        target,
        link,
        span: None,
        inventory: InventorySelector::Any,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn any(display: Option<&str>, target: &str, link: bool) -> InlineNode {
        InlineNode::AnyReference {
            display: display.map(str::to_string),
            target: target.to_string(),
            link,
            span: None,
            inventory: InventorySelector::Any,
        }
    }

    #[test]
    fn test_handle_any_match_leaves_a_bare_target_untitled() {
        // Given / When
        let node = handle_any_match(":any:`pkg.run`");

        // Then
        assert_eq!(node, any(None, "pkg.run", true));
    }

    #[test]
    fn test_handle_any_match_splits_an_explicit_title() {
        // Given / When
        let node = handle_any_match(":any:`Installing <install>`");

        // Then
        assert_eq!(node, any(Some("Installing"), "install", true));
    }

    #[test]
    fn test_handle_any_match_turns_a_bang_prefix_into_an_unlinked_literal() {
        // Given / When
        let node = handle_any_match(":any:`!pkg.run`");

        // Then
        assert_eq!(node, any(None, "pkg.run", false));
    }

    #[test]
    fn test_handle_any_match_strips_the_bang_before_splitting_a_title() {
        // Given / When
        let node = handle_any_match(":any:`!Run it <pkg.run>`");

        // Then
        assert_eq!(node, any(Some("Run it"), "pkg.run", false));
    }

    #[test]
    fn test_handle_any_match_keeps_a_tilde_and_call_parens_in_the_target() {
        // Given — neither is markup to `:any:` itself
        // When
        let tilde = handle_any_match(":any:`~pkg.run`");
        let parens = handle_any_match(":any:`run()`");

        // Then
        assert_eq!(tilde, any(None, "~pkg.run", true));
        assert_eq!(parens, any(None, "run()", true));
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
    fn test_parse_places_an_any_role_at_its_source_position() {
        // Given / When
        let doc = crate::parse("test.rst", "See :any:`install` here.");

        // Then
        let rinx_ast::Node::Paragraph(inlines) = &doc.nodes[0] else {
            panic!("expected a paragraph, got {:?}", doc.nodes[0]);
        };
        assert_eq!(
            inlines[1].span(),
            Some(rinx_ast::Span::new(
                rinx_ast::Position::new(1, 5),
                rinx_ast::Position::new(1, 19)
            ))
        );
    }

    #[test]
    fn test_parse_reads_an_external_prefix_on_an_any_role() {
        // Given / When
        let external = only_inline(":external:any:`dict`");
        let named = only_inline(":external+python:any:`dict`");

        // Then
        assert!(matches!(
            external,
            InlineNode::AnyReference {
                inventory: InventorySelector::ExternalOnly,
                ..
            }
        ));
        assert!(matches!(
            named,
            InlineNode::AnyReference {
                inventory: InventorySelector::Named(_),
                ..
            }
        ));
    }

    #[test]
    fn test_parse_unescapes_an_any_role_target() {
        // Given — an escaped `!` is text, not the suppression prefix
        // When
        let node = only_inline(r":any:`\!odd`");

        // Then
        assert!(matches!(
            node,
            InlineNode::AnyReference { ref target, link: true, .. } if target == "!odd"
        ));
    }
}
