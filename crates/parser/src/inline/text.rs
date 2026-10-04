//! The inline scan itself: walking a paragraph's text, picking the earliest
//! match among every role, link and markup candidate, and stripping escape
//! markers off the nodes on the way out.

use rinx_ast::{Domain, InlineNode};

use super::dispatch::handle_inline_match;
use super::escapes::{EscapedText, unescape, unescape_keeping_backslashes};
use super::interpreted::{handle_interpreted_text, prefix_role_trailer, refused};
use super::markup::{InlineMarkup, find_inline_markup};
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
                all_matches.push((m.start(), m.end(), Candidate::Role(kind)));
            }
        }
        if let Some((start, end, markup)) = find_inline_markup(paragraph_text, last_match_end) {
            all_matches.push((start, end, Candidate::Markup(markup)));
        }

        let earliest = all_matches
            .into_iter()
            .min_by_key(|(start, end, _)| (*start, std::cmp::Reverse(*end)));

        if let Some((start, mut end, candidate)) = earliest {
            if start > 0 {
                inlines.push(unescape_node(InlineNode::Text(apply_smart_typography(
                    &remaining[..start],
                ))));
            }
            let node = match candidate {
                Candidate::Role(kind) => match role_trailer(kind, &remaining[end..]) {
                    Some((refusal, length)) => {
                        end += length;
                        refused(&remaining[start..end], refusal)
                    }
                    None => {
                        handle_inline_match(kind, &remaining[start..end], None, default_domain, ctx)
                    }
                },
                Candidate::Markup(InlineMarkup::Node(node)) => node,
                Candidate::Markup(InlineMarkup::Interpreted { body, suffix_role }) => {
                    handle_interpreted_text(
                        &remaining[start..end],
                        &body,
                        suffix_role.as_deref(),
                        default_domain,
                        ctx,
                    )
                }
            };
            let span = map.span(last_match_end + start, last_match_end + end, ctx);
            inlines.push(unescape_node(node).with_span(span));
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

/// One construct the scan could read next: a role or link from the role
/// table, by its `kind` tag, or what the markup scan found.
enum Candidate {
    Role(&'static str),
    Markup(InlineMarkup),
}

/// What a role written before its text is followed by, when it is something
/// that role cannot also have — see [`prefix_role_trailer`]. Only a role's
/// match is asked: a link's backquotes are not interpreted text.
fn role_trailer(kind: &str, after: &str) -> Option<(rinx_ast::RoleRefusal, usize)> {
    if matches!(kind, "phrased" | "simple" | "anon_phrased" | "anon_simple") {
        return None;
    }
    prefix_role_trailer(after)
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
/// [`InlineNode::Literal`] and [`InlineNode::Math`] are the verbatim
/// contexts, so their markers turn back into backslashes; every other field —
/// [`InlineNode::Code`]'s included — takes the display form, in which an
/// escaped space disappears entirely.
fn unescape_node(mut node: InlineNode) -> InlineNode {
    match &mut node {
        InlineNode::Literal(content) => *content = unescape_keeping_backslashes(content),
        InlineNode::Text(text)
        | InlineNode::Emphasis(text)
        | InlineNode::Strong(text)
        | InlineNode::Program(text)
        | InlineNode::Script { text, .. }
        | InlineNode::TitleReference(text)
        // Unlike an inline literal, `:code:` is interpreted text: Sphinx's
        // `code_role` is handed it escaped, so `\*` shows `*` and `\\` a
        // single backslash.
        | InlineNode::Code { text, .. }
        | InlineNode::AnonymousReference { text, .. }
        // A hyperlink's target was unescaped while it was read, since only
        // the escaped form shows whether a trailing underscore makes it an
        // alias (see `link_destination`), so only the text is left.
        | InlineNode::Hyperlink { text, .. }
        | InlineNode::AnonymousHyperlink { text, .. }
        // A `:numref:` title was unescaped before it was parsed into a format
        // (see `roles::numref`), so only the label and a refusal's text are
        // left. A refusal's own fields were unescaped before it was refused.
        | InlineNode::NumberReference { target: text, .. }
        | InlineNode::RefusedRole { text, .. } => *text = unescape(text),
        // The target was unescaped before it was parsed (see
        // `roles::registry`), so only the title is left.
        InlineNode::RegistryReference { display, .. } => {
            *display = display.as_deref().map(unescape);
        }
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
        // A `:pep-reference:`'s or `:rfc-reference:`'s number was unescaped
        // before it was parsed (see `roles::pep_reference`), and it is all
        // the node holds; an `:index:`'s title and target were both unescaped
        // before the target was parsed into entries (see `roles::index`). An
        // inline image is never produced by the inline
        // scan itself — only by the whole-document substitution resolver
        // splicing an already-built node in after this function has already
        // run on it once.
        InlineNode::DocutilsPepReference { .. }
        | InlineNode::DocutilsRfcReference { .. }
        | InlineNode::IndexReference { .. }
        | InlineNode::InlineImage(_) => {}
    }
    node
}

#[cfg(test)]
mod tests {
    use rinx_ast::RoleRefusal;

    use super::*;

    #[test]
    fn test_role_trailer_refuses_a_second_role_after_a_role() {
        // Given / When / Then
        assert_eq!(
            role_trailer("script", ":sup:"),
            Some((RoleRefusal::MultipleRoles, 5))
        );
    }

    #[test]
    fn test_role_trailer_ignores_what_follows_a_link() {
        // Given / When / Then
        for kind in ["phrased", "simple", "anon_phrased", "anon_simple"] {
            assert_eq!(role_trailer(kind, ":sup:"), None, "{kind}");
        }
    }
}
