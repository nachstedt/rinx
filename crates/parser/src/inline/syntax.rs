//! Recording the names an inline construct writes, for an editor colouring
//! them — see [`rinx_ast::SyntaxTable`]: a role's `:name:`, what a reference
//! leads to, a `|substitution|`.
//!
//! Read off the text the scan matched, once it knows what node the match
//! made, so only a construct the scan read as a role or reference is
//! coloured as one.

use std::ops::Range;

use rinx_ast::{InlineNode, TokenKind, TokenModifiers};

use super::source_map::SourceMap;
use crate::context::ParseCtx;
use crate::explicit_title::role_target_range;

/// Records the names in `matched`, which the scan read into `node` at byte
/// `offset` of the text `map` positions.
pub(super) fn record_inline_syntax(
    matched: &str,
    node: &InlineNode,
    offset: usize,
    map: &SourceMap,
    ctx: &ParseCtx<'_>,
) {
    for (kind, range) in inline_tokens(matched, node) {
        let span = map.span(offset + range.start, offset + range.end, ctx);
        ctx.record_token(kind, TokenModifiers::NONE, span);
    }
}

/// The names `matched`, read into `node`, writes, as byte ranges of it.
fn inline_tokens(matched: &str, node: &InlineNode) -> Vec<(TokenKind, Range<usize>)> {
    if matches!(node, InlineNode::SubstitutionReference { .. }) {
        return vec![(TokenKind::Substitution, substitution_range(matched))];
    }
    let mut tokens = Vec::new();
    let Some((open, close)) = backquotes(matched) else {
        // A bare `name_` or `name__`: the name is the target.
        if leads_somewhere(node) {
            let name = matched.trim_end_matches('_');
            tokens.push((TokenKind::Reference, 0..name.len()));
        }
        return tokens;
    };
    if matched.starts_with(':') && matched[..open].ends_with(':') {
        tokens.push((TokenKind::Role, 0..open));
    }
    if leads_somewhere(node) {
        let content = &matched[open + 1..close];
        let chars = role_target_range(content);
        let byte = |char_index: usize| {
            content
                .char_indices()
                .nth(char_index)
                .map_or(content.len(), |(at, _)| at)
        };
        let target = open + 1 + byte(chars.start)..open + 1 + byte(chars.end);
        if !target.is_empty() {
            tokens.push((TokenKind::Reference, target));
        }
    }
    let after = &matched[close + 1..];
    if after.len() > 1 && after.starts_with(':') && after.ends_with(':') {
        tokens.push((TokenKind::Role, close + 1..matched.len()));
    }
    tokens
}

/// The byte offsets of the first and last backquote of `matched`, when it
/// holds two.
fn backquotes(matched: &str) -> Option<(usize, usize)> {
    let open = matched.find('`')?;
    let close = matched.rfind('`')?;
    (close > open).then_some((open, close))
}

/// A substitution's `|name|`, without the `_` or `__` that makes it a
/// reference too.
fn substitution_range(matched: &str) -> Range<usize> {
    0..matched.trim_end_matches('_').len()
}

/// Whether `node` leads somewhere — a cross-reference or a hyperlink —
/// rather than only marking text up.
const fn leads_somewhere(node: &InlineNode) -> bool {
    matches!(
        node,
        InlineNode::Reference { .. }
            | InlineNode::Hyperlink { .. }
            | InlineNode::AnonymousReference { .. }
            | InlineNode::AnonymousHyperlink { .. }
            | InlineNode::TermReference { .. }
            | InlineNode::DomainObjectReference { .. }
            | InlineNode::OptionReference { .. }
            | InlineNode::AnyReference { .. }
            | InlineNode::DocReference { .. }
            | InlineNode::NumberReference { .. }
            | InlineNode::RegistryReference { .. }
            | InlineNode::DocutilsPepReference { .. }
            | InlineNode::DocutilsRfcReference { .. }
            | InlineNode::DownloadReference { .. }
            | InlineNode::EntityReference { .. }
            | InlineNode::EquationReference { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rinx_ast::{Position, Span, SyntaxToken};

    /// The tokens a paragraph of `text` on line 1 records.
    fn tokens(text: &str) -> Vec<(TokenKind, u32, u32)> {
        parse("test.rst", text)
            .syntax
            .tokens
            .iter()
            .map(|token| (token.kind, token.span.start.column, token.span.end.column))
            .collect()
    }

    #[test]
    fn test_a_prefix_role_records_its_name_and_target() {
        // When
        let found = tokens("See :ref:`install`.");

        // Then
        assert_eq!(
            found,
            vec![(TokenKind::Role, 5, 10), (TokenKind::Reference, 11, 18)]
        );
    }

    #[test]
    fn test_an_explicit_title_records_only_the_target_as_the_reference() {
        // When
        let found = tokens(":doc:`the guide <guide>`");

        // Then
        assert_eq!(
            found,
            vec![(TokenKind::Role, 1, 6), (TokenKind::Reference, 18, 23)]
        );
    }

    #[test]
    fn test_a_domain_role_records_its_whole_name() {
        // When
        let found = tokens(":py:func:`open`");

        // Then
        assert_eq!(
            found,
            vec![(TokenKind::Role, 1, 10), (TokenKind::Reference, 11, 15)]
        );
    }

    #[test]
    fn test_a_suffix_role_records_its_name_after_the_text() {
        // When
        let found = tokens("`install`:ref:");

        // Then
        assert_eq!(
            found,
            vec![(TokenKind::Reference, 2, 9), (TokenKind::Role, 10, 15)]
        );
    }

    #[test]
    fn test_a_role_that_leads_nowhere_records_only_its_name() {
        // When
        let found = tokens(":math:`x^2`");

        // Then
        assert_eq!(found, vec![(TokenKind::Role, 1, 7)]);
    }

    #[test]
    fn test_a_named_reference_records_its_name_without_the_underscore() {
        // When
        let found = tokens("Read install_ now.");

        // Then
        assert_eq!(found, vec![(TokenKind::Reference, 6, 13)]);
    }

    #[test]
    fn test_a_phrased_hyperlink_records_its_address() {
        // When
        let found = tokens("`Python <https://python.org>`_");

        // Then
        assert_eq!(found, vec![(TokenKind::Reference, 10, 28)]);
    }

    #[test]
    fn test_a_substitution_reference_is_recorded_before_it_is_resolved() {
        // When
        let document = parse("test.rst", "A |name| here.\n\n.. |name| replace:: text\n");

        // Then
        assert!(document.syntax.tokens.contains(&SyntaxToken {
            kind: TokenKind::Substitution,
            modifiers: TokenModifiers::NONE,
            span: Span::new(Position::new(1, 3), Position::new(1, 9)),
        }));
    }

    #[test]
    fn test_an_inline_literal_records_nothing() {
        // When / Then
        assert_eq!(tokens("``:ref:`x```"), Vec::new());
    }

    #[test]
    fn test_substitution_range_drops_a_reference_underscore() {
        // When / Then
        assert_eq!(substitution_range("|logo|__"), 0..6);
    }

    #[test]
    fn test_backquotes_needs_two() {
        // When / Then
        assert_eq!(backquotes("a`b`c"), Some((1, 3)));
        assert_eq!(backquotes("a`b"), None);
    }
}
