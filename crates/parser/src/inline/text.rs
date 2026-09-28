//! The inline scan itself: walking a paragraph's text, picking the earliest
//! match among every role, link and markup candidate, and stripping escape
//! markers off the nodes on the way out.

use rinx_ast::{Domain, InlineNode};

use super::dispatch::handle_inline_match;
use super::escapes::{EscapedText, unescape, unescape_keeping_backslashes};
use super::markup::find_inline_markup;
use super::regexes::SIMPLE_ROLE_REGEXES;
use super::source_map::SourceMap;
use super::typography::apply_smart_typography;
use crate::context::ParseCtx;

/// Parses a plain text string into a list of [`InlineNode`]s, recording where
/// each cross-reference role was written.
///
/// `default_domain` resolves any bare (unprefixed) domain role, e.g. `:func:`,
/// to a concrete [`Domain`] — mirroring how bare directives are resolved.
///
/// The positions come from `map`, which the caller built while it reflowed the
/// source into `raw_text`; `ctx` is what turns the map's own line/column
/// offsets into absolute ones. Spans land on the nodes here rather than inside
/// each role parser because this is the only place a match's extent is known:
/// `start`/`end` below are exactly it, and the escape rewrite is deliberately
/// length-preserving so they still address the original text.
pub(crate) fn parse_inline_text_mapped(
    raw_text: &str,
    default_domain: Domain,
    map: &SourceMap,
    ctx: &ParseCtx<'_>,
) -> Vec<InlineNode> {
    // Rewrite escapes to markers once, up front, and match markup over that
    // form for the rest of the function. The rewrite preserves byte lengths,
    // so every offset below means the same thing it did before.
    let escaped = EscapedText::new(raw_text);
    let paragraph_text = escaped.as_str();

    let mut inlines = Vec::new();
    let mut last_match_end = 0;

    while last_match_end < paragraph_text.len() {
        let remaining = &paragraph_text[last_match_end..];

        let mut all_matches = Vec::new();
        for (regex, kind) in SIMPLE_ROLE_REGEXES {
            if let Some(m) = regex.find(remaining) {
                all_matches.push((m.start(), m.end(), *kind, None));
            }
        }
        if let Some((start, end, node)) = find_inline_markup(paragraph_text, last_match_end) {
            all_matches.push((start, end, "inline", Some(node)));
        }

        let earliest = all_matches
            .into_iter()
            .min_by_key(|(start, end, _, _)| (*start, std::cmp::Reverse(*end)));

        if let Some((start, end, kind, node_opt)) = earliest {
            if start > 0 {
                inlines.push(unescape_node(InlineNode::Text(apply_smart_typography(
                    &remaining[..start],
                ))));
            }
            let m_str = &remaining[start..end];
            let span = map.span(last_match_end + start, last_match_end + end, ctx);
            inlines.push(
                unescape_node(handle_inline_match(
                    kind,
                    m_str,
                    node_opt,
                    default_domain,
                    ctx.schema,
                ))
                .with_span(span),
            );
            last_match_end += end;
        } else {
            inlines.push(unescape_node(InlineNode::Text(apply_smart_typography(
                remaining,
            ))));
            break;
        }
    }
    inlines
}

/// Strips escape markers from every text field of a node on its way out of
/// [`parse_inline_text_mapped`].
///
/// Every node is funnelled through here rather than unescaped at each of the
/// two dozen places one gets built, because that turns the "no marker ever
/// reaches the AST" guarantee into something the compiler checks: the match is
/// exhaustive, so a variant added later cannot quietly start leaking markers
/// into the rendered HTML.
///
/// Smart typography has already run by this point, and deliberately so — it
/// sees the escaped form, which is why `\-\-` stays two hyphens instead of
/// becoming an en dash, matching docutils' smartquotes transform.
///
/// [`InlineNode::Literal`] is the one verbatim context, so its markers turn
/// back into backslashes; every other field takes the display form, in which
/// an escaped space disappears entirely.
fn unescape_node(mut node: InlineNode) -> InlineNode {
    match &mut node {
        InlineNode::Literal(content) => *content = unescape_keeping_backslashes(content),
        InlineNode::Text(text)
        | InlineNode::Emphasis(text)
        | InlineNode::Strong(text)
        | InlineNode::Program(text)
        | InlineNode::AnonymousReference { text, .. } => *text = unescape(text),
        InlineNode::Reference {
            display, target, ..
        }
        | InlineNode::AnyReference {
            display, target, ..
        }
        | InlineNode::DocReference {
            display, target, ..
        } => {
            *display = display.as_deref().map(unescape);
            *target = unescape(target);
        }
        // The file is re-split after unescaping, so an escaped character can
        // never decide whether it names a URL or a project file.
        InlineNode::DownloadReference {
            display, target, ..
        } => {
            *display = display.as_deref().map(unescape);
            *target = rinx_ast::AssetUri::new(&unescape(target.as_written()));
        }
        InlineNode::EntityReference {
            display, target, ..
        }
        | InlineNode::Hyperlink {
            text: display,
            target,
            ..
        }
        | InlineNode::AnonymousHyperlink {
            text: display,
            target,
        }
        | InlineNode::TermReference {
            display,
            term: target,
            ..
        }
        | InlineNode::OptionReference {
            display, target, ..
        }
        | InlineNode::DomainObjectReference {
            display,
            name: target,
            ..
        } => {
            *display = unescape(display);
            *target = unescape(target);
        }
        // LaTeX is a verbatim context, like `InlineNode::Literal`: its
        // backslashes are content (`\alpha`, `\\`), so the markers turn back
        // into backslashes rather than being dropped.
        InlineNode::Math { latex, .. } => *latex = unescape_keeping_backslashes(latex),
        // An equation label and a substitution name are identifiers, not
        // LaTeX or prose, so they take the display form like every other
        // cross-reference target.
        InlineNode::EquationReference { label: name, .. }
        | InlineNode::SubstitutionReference { name, .. } => *name = unescape(name),
        // Never produced by the inline scan itself — only by the
        // whole-document substitution resolver splicing an already-built
        // node in after this function has already run on it once.
        InlineNode::InlineImage(_) => {}
    }
    node
}
