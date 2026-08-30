use crate::escapes::{EscapedText, is_escaped_at, unescape, unescape_keeping_backslashes};
use crate::punctuation::{can_follow_end_string, can_precede_start_string};
use crate::typography::apply_smart_typography;
use regex::Regex;
use rusty_sphinx_ast::{Domain, InlineNode};
use std::sync::LazyLock;

static REF_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":ref:`(?P<target>[^`]+)`").unwrap());
static PROGRAM_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":program:`(?P<name>[^`]+)`").unwrap());
static TERM_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":term:`(?P<content>[^`]+)`").unwrap());
static OPTION_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":option:`(?P<content>[^`]+)`").unwrap());
pub(super) static FUNC_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(?:(?P<domain>py|c):)?func:`(?P<name>[^`]+)`").unwrap());
pub(super) static MOD_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(?:(?P<domain>py):)?mod:`(?P<name>[^`]+)`").unwrap());
pub(super) static DATA_ROLE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r":(?:(?P<domain>py|c):)?(?P<role>data|const|var|member):`(?P<name>[^`]+)`").unwrap()
});
pub(super) static METH_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(?:(?P<domain>py):)?meth:`(?P<name>[^`]+)`").unwrap());
pub(super) static CLASS_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(?:(?P<domain>py):)?class:`(?P<name>[^`]+)`").unwrap());
pub(super) static ATTR_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(?:(?P<domain>py):)?attr:`(?P<name>[^`]+)`").unwrap());
pub(super) static EXC_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(?:(?P<domain>py):)?exc:`(?P<name>[^`]+)`").unwrap());
pub(super) static MACRO_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(?:(?P<domain>c):)?macro:`(?P<name>[^`]+)`").unwrap());
pub(super) static STRUCT_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(?:(?P<domain>c):)?struct:`(?P<name>[^`]+)`").unwrap());
pub(super) static UNION_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(?:(?P<domain>c):)?union:`(?P<name>[^`]+)`").unwrap());
pub(super) static TYPE_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(?:(?P<domain>c):)?type:`(?P<name>[^`]+)`").unwrap());
static PHRASED_LINK_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`(?P<text>[^`]+)`_").unwrap());
static SIMPLE_LINK_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(?P<name>[a-zA-Z0-9_.-]+)_\b").unwrap());
static EMBEDDED_URI_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?P<text>.*)\s+<(?P<uri>[^>]+)>$").unwrap());
static ANONYMOUS_PHRASED_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`(?P<text>[^`]+)`__").unwrap());
static ANONYMOUS_SIMPLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(?P<name>[a-zA-Z0-9_.-]+)__\b").unwrap());

/// Every role/link regex tried by [`parse_inline_text`], paired with the
/// `kind` tag [`handle_inline_match`] dispatches on. A table rather than one
/// `let X_match = ...; if let Some(m) = X_match { ... }` pair per regex
/// (which is what this used to be, and grew one clippy line-count warning
/// past its limit the moment a 20th regex — `:option:`'s — joined it): match
/// order here doesn't matter, since [`parse_inline_text`] always picks the
/// earliest (then longest) match regardless of table position.
static SIMPLE_ROLE_REGEXES: &[(&LazyLock<Regex>, &str)] = &[
    (&REF_REGEX, "ref"),
    (&PROGRAM_ROLE_REGEX, "program"),
    (&TERM_ROLE_REGEX, "term"),
    (&OPTION_ROLE_REGEX, "option"),
    (&FUNC_ROLE_REGEX, "func"),
    (&MOD_ROLE_REGEX, "mod"),
    (&DATA_ROLE_REGEX, "data"),
    (&METH_ROLE_REGEX, "meth"),
    (&CLASS_ROLE_REGEX, "class"),
    (&ATTR_ROLE_REGEX, "attr"),
    (&EXC_ROLE_REGEX, "exc"),
    (&MACRO_ROLE_REGEX, "macro"),
    (&STRUCT_ROLE_REGEX, "struct"),
    (&UNION_ROLE_REGEX, "union"),
    (&TYPE_ROLE_REGEX, "type"),
    (&ANONYMOUS_PHRASED_REGEX, "anon_phrased"),
    (&PHRASED_LINK_REGEX, "phrased"),
    (&ANONYMOUS_SIMPLE_REGEX, "anon_simple"),
    (&SIMPLE_LINK_REGEX, "simple"),
];

/// Parses a plain text string into a list of [`InlineNode`]s.
///
/// `default_domain` resolves any bare (unprefixed) domain role, e.g. `:func:`,
/// to a concrete [`Domain`] — mirroring how bare directives are resolved.
pub(super) fn parse_inline_text(raw_text: &str, default_domain: Domain) -> Vec<InlineNode> {
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
            inlines.push(unescape_node(handle_inline_match(
                kind,
                m_str,
                node_opt,
                default_domain,
            )));
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
/// [`parse_inline_text`].
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
fn unescape_node(node: InlineNode) -> InlineNode {
    match node {
        InlineNode::Literal(content) => InlineNode::Literal(unescape_keeping_backslashes(&content)),
        InlineNode::Text(text) => InlineNode::Text(unescape(&text)),
        InlineNode::Emphasis(text) => InlineNode::Emphasis(unescape(&text)),
        InlineNode::Strong(text) => InlineNode::Strong(unescape(&text)),
        InlineNode::Program(name) => InlineNode::Program(unescape(&name)),
        InlineNode::AnonymousReference(text) => InlineNode::AnonymousReference(unescape(&text)),
        InlineNode::Reference { display, target } => InlineNode::Reference {
            display: unescape(&display),
            target: unescape(&target),
        },
        InlineNode::Hyperlink { text, target } => InlineNode::Hyperlink {
            text: unescape(&text),
            target: unescape(&target),
        },
        InlineNode::AnonymousHyperlink { text, target } => InlineNode::AnonymousHyperlink {
            text: unescape(&text),
            target: unescape(&target),
        },
        InlineNode::TermReference { display, term } => InlineNode::TermReference {
            display: unescape(&display),
            term: unescape(&term),
        },
        InlineNode::OptionReference { display, target } => InlineNode::OptionReference {
            display: unescape(&display),
            target: unescape(&target),
        },
        InlineNode::DomainObjectReference {
            object_type,
            name,
            display,
            link,
            search_order,
        } => InlineNode::DomainObjectReference {
            object_type,
            name: unescape(&name),
            display: unescape(&display),
            link,
            search_order,
        },
    }
}

mod domain_roles;
mod domain_target;

use domain_roles::c::{
    handle_macro_match, handle_struct_match, handle_type_match, handle_union_match,
};
use domain_roles::py::{
    handle_attr_match, handle_class_match, handle_data_match, handle_exc_match, handle_func_match,
    handle_meth_match, handle_mod_match,
};

/// Splits a role's backtick content on Sphinx's optional explicit-title
/// syntax (`Display text <target>`), shared by every role that supports it
/// (`:term:`, `:ref:`, and the domain-object roles via
/// [`parse_domain_object_target`]). Returns `None` when there is no explicit
/// title.
pub(super) fn split_explicit_title(content: &str) -> Option<(String, String)> {
    let angle_start = content.rfind('<')?;
    let angle_end = content[angle_start..].find('>')?;
    let display = content[..angle_start].trim().to_string();
    let target = content[angle_start + 1..angle_start + angle_end]
        .trim()
        .to_string();
    Some((display, target))
}

/// Splits a role's backtick content on Sphinx's optional explicit-title
/// syntax, shared by `:term:`/`:ref:`. Returns `(display, target)`, both
/// equal to `content` when there is no explicit title.
fn split_display_and_target(content: &str) -> (String, String) {
    split_explicit_title(content).unwrap_or_else(|| (content.to_string(), content.to_string()))
}

pub(super) fn handle_inline_match(
    kind: &str,
    m_str: &str,
    node_opt: Option<InlineNode>,
    default_domain: Domain,
) -> InlineNode {
    match kind {
        "inline" => node_opt.expect("inline node should be present"),
        "ref" => {
            let caps = REF_REGEX.captures(m_str).unwrap();
            let (display, target) = split_display_and_target(&caps["target"]);
            InlineNode::Reference { display, target }
        }
        "program" => {
            let caps = PROGRAM_ROLE_REGEX.captures(m_str).unwrap();
            InlineNode::Program(caps["name"].to_string())
        }
        "func" => handle_func_match(m_str, default_domain),
        "mod" => handle_mod_match(m_str, default_domain),
        "data" => handle_data_match(m_str, default_domain),
        "meth" => handle_meth_match(m_str, default_domain),
        "class" => handle_class_match(m_str, default_domain),
        "attr" => handle_attr_match(m_str, default_domain),
        "exc" => handle_exc_match(m_str, default_domain),
        "macro" => handle_macro_match(m_str, default_domain),
        "struct" => handle_struct_match(m_str, default_domain),
        "union" => handle_union_match(m_str, default_domain),
        "type" => handle_type_match(m_str, default_domain),
        "term" => {
            let caps = TERM_ROLE_REGEX.captures(m_str).unwrap();
            let (display, term) = split_display_and_target(&caps["content"]);
            InlineNode::TermReference { display, term }
        }
        "option" => {
            let caps = OPTION_ROLE_REGEX.captures(m_str).unwrap();
            let (display, target) = split_display_and_target(&caps["content"]);
            InlineNode::OptionReference { display, target }
        }
        "phrased" => {
            let caps = PHRASED_LINK_REGEX.captures(m_str).unwrap();
            let text_full = &caps["text"];
            if let Some(embedded) = EMBEDDED_URI_REGEX.captures(text_full) {
                InlineNode::Hyperlink {
                    text: embedded["text"].trim().to_string(),
                    target: embedded["uri"].to_string(),
                }
            } else {
                InlineNode::Hyperlink {
                    text: text_full.to_string(),
                    target: text_full.to_string(),
                }
            }
        }
        "simple" => {
            let caps = SIMPLE_LINK_REGEX.captures(m_str).unwrap();
            let name = &caps["name"];
            InlineNode::Hyperlink {
                text: name.to_string(),
                target: name.to_string(),
            }
        }
        "anon_phrased" => {
            let caps = ANONYMOUS_PHRASED_REGEX.captures(m_str).unwrap();
            let text_full = &caps["text"];
            if let Some(embedded) = EMBEDDED_URI_REGEX.captures(text_full) {
                InlineNode::AnonymousHyperlink {
                    text: embedded["text"].trim().to_string(),
                    target: embedded["uri"].to_string(),
                }
            } else {
                InlineNode::AnonymousReference(text_full.to_string())
            }
        }
        "anon_simple" => {
            let caps = ANONYMOUS_SIMPLE_REGEX.captures(m_str).unwrap();
            let name = &caps["name"];
            InlineNode::AnonymousReference(name.to_string())
        }
        _ => unreachable!(),
    }
}

/// Returns every absolute byte position in `full_text` at or after
/// `search_start` where `marker` occurs and could validly serve as a
/// *closing* marker for some inline-markup span: not preceded by whitespace,
/// followed by whitespace/allowed punctuation (or end of text), and (unless
/// `is_literal`) not itself escaped by a preceding backslash.
///
/// Whether an occurrence of `marker` is valid as a closing marker depends
/// only on the text around that occurrence, never on where a candidate
/// opening marker started searching from. Computing this table once per
/// `marker` (here, once per call to [`find_inline_markup`]) instead of
/// rescanning it from scratch for every failed candidate opening marker is
/// what keeps inline-markup parsing from being quadratic in input size.
fn find_valid_close_positions(
    full_text: &str,
    search_start: usize,
    marker: &str,
    is_literal: bool,
) -> Vec<usize> {
    let marker_len = marker.len();
    let mut positions = Vec::new();
    let mut search_pos = search_start;

    while let Some(rel_pos) = full_text[search_pos..].find(marker) {
        let abs_pos = search_pos + rel_pos;

        // End context check
        let last_inner = full_text[..abs_pos].chars().next_back();
        if last_inner.is_some_and(char::is_whitespace) {
            search_pos = abs_pos + 1;
            continue;
        }

        // Check character after end marker.
        let after_end = abs_pos + marker_len;
        if after_end < full_text.len() {
            let next_char = full_text[after_end..].chars().next().unwrap();
            if !can_follow_end_string(next_char) {
                search_pos = abs_pos + 1;
                continue;
            }
        }

        // An escaped end marker closes nothing. Literals are exempt, mirroring
        // docutils using `non_whitespace_before` for them where emphasis and
        // strong use `non_whitespace_escape_before`.
        if !is_literal && is_escaped_at(full_text, abs_pos) {
            search_pos = abs_pos + 1;
            continue;
        }

        positions.push(abs_pos);
        search_pos = abs_pos + 1;
    }

    positions
}

pub(super) fn find_inline_markup(
    full_text: &str,
    start_offset: usize,
) -> Option<(usize, usize, InlineNode)> {
    let text = &full_text[start_offset..];
    let mut best_match: Option<(usize, usize, InlineNode)> = None;

    let literal_close_positions = find_valid_close_positions(full_text, start_offset, "``", true);
    let strong_close_positions = find_valid_close_positions(full_text, start_offset, "**", false);
    let emphasis_close_positions = find_valid_close_positions(full_text, start_offset, "*", false);

    for (i, _) in text.char_indices() {
        let abs_i = start_offset + i;

        // An escaped character never opens markup.
        if is_escaped_at(full_text, abs_i) {
            continue;
        }

        // Try Inline Literal first (``)
        if text[i..].starts_with("``")
            && let Some((end_pos, content)) =
                try_match_inline(full_text, abs_i, 2, &literal_close_positions)
        {
            let m = (i, i + (end_pos - abs_i), InlineNode::Literal(content));
            if best_match.is_none() || m.0 < best_match.as_ref().unwrap().0 {
                best_match = Some(m);
                break; // Found the earliest match
            }
        }

        // Try Strong Emphasis next (**)
        if text[i..].starts_with("**")
            && let Some((end_pos, content)) =
                try_match_inline(full_text, abs_i, 2, &strong_close_positions)
        {
            let m = (
                i,
                i + (end_pos - abs_i),
                InlineNode::Strong(apply_smart_typography(&content)),
            );
            if best_match.is_none() || m.0 < best_match.as_ref().unwrap().0 {
                best_match = Some(m);
                break; // Found the earliest match
            }
        }

        // Try Emphasis (*)
        if text[i..].starts_with('*')
            && !text[i..].starts_with("**")
            && let Some((end_pos, content)) =
                try_match_inline(full_text, abs_i, 1, &emphasis_close_positions)
        {
            let m = (
                i,
                i + (end_pos - abs_i),
                InlineNode::Emphasis(apply_smart_typography(&content)),
            );
            if best_match.is_none() || m.0 < best_match.as_ref().unwrap().0 {
                best_match = Some(m);
                break; // Found the earliest match
            }
        }
    }

    best_match
}

pub(super) fn try_match_inline(
    full_text: &str,
    start_pos: usize,
    marker_len: usize,
    close_positions: &[usize],
) -> Option<(usize, String)> {
    // Start context check
    if start_pos > 0 {
        let prev_char = full_text[..start_pos].chars().next_back().unwrap();
        if !can_precede_start_string(prev_char) {
            return None;
        }
    }

    let after_start = start_pos + marker_len;
    if after_start >= full_text.len() {
        return None;
    }

    let first_inner = full_text[after_start..].chars().next().unwrap();
    if first_inner.is_whitespace() {
        return None;
    }

    // The closing search only ever considers positions at or after
    // `search_pos`, which is always strictly after `after_start` (it skips
    // past the first inner character), so any matched close position yields
    // non-empty content — no separate empty-content check is needed.
    let search_pos = after_start + first_inner.len_utf8();
    let idx = close_positions.partition_point(|&p| p < search_pos);
    let &abs_end_pos = close_positions.get(idx)?;
    let content = full_text[after_start..abs_end_pos].to_string();

    Some((abs_end_pos + marker_len, content))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Escapes `raw` the way [`parse_inline_text`] does before any of the
    /// helpers below see it, so a unit test exercises the form those helpers
    /// are actually handed rather than a raw backslash they never meet.
    fn escaped(raw: &str) -> String {
        crate::escapes::EscapedText::new(raw).as_str().to_string()
    }

    #[test]
    fn test_handle_inline_match_inline_variant() {
        // Given
        let node = InlineNode::Emphasis("text".to_string());
        // When
        let result = handle_inline_match("inline", "", Some(node.clone()), Domain::Py);
        // Then
        assert_eq!(result, node);
    }
    #[test]
    fn test_handle_inline_match_ref_variant() {
        let result = handle_inline_match("ref", ":ref:`target`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::Reference {
                display: "target".to_string(),
                target: "target".to_string(),
            }
        );
    }
    #[test]
    fn test_handle_inline_match_ref_variant_with_display_text() {
        let result = handle_inline_match(
            "ref",
            ":ref:`GenericAlias <types-genericalias>`",
            None,
            Domain::Py,
        );
        assert_eq!(
            result,
            InlineNode::Reference {
                display: "GenericAlias".to_string(),
                target: "types-genericalias".to_string(),
            }
        );
    }
    #[test]
    fn test_handle_inline_match_program_variant() {
        let result = handle_inline_match("program", ":program:`curl`", None, Domain::Py);
        assert_eq!(result, InlineNode::Program("curl".to_string()));
    }
    #[test]
    fn test_handle_inline_match_phrased_with_embedded_uri() {
        let result = handle_inline_match("phrased", "`text <http://uri>`_", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::Hyperlink {
                text: "text".to_string(),
                target: "http://uri".to_string(),
            }
        );
    }
    #[test]
    fn test_handle_inline_match_phrased_without_uri() {
        let result = handle_inline_match("phrased", "`just text`_", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::Hyperlink {
                text: "just text".to_string(),
                target: "just text".to_string(),
            }
        );
    }
    #[test]
    fn test_handle_inline_match_simple_variant() {
        let result = handle_inline_match("simple", "name_", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::Hyperlink {
                text: "name".to_string(),
                target: "name".to_string(),
            }
        );
    }
    #[test]
    fn test_handle_inline_match_anon_phrased_with_embedded_uri() {
        let result = handle_inline_match("anon_phrased", "`text <http://uri>`__", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::AnonymousHyperlink {
                text: "text".to_string(),
                target: "http://uri".to_string(),
            }
        );
    }
    #[test]
    fn test_handle_inline_match_anon_phrased_without_uri() {
        let result = handle_inline_match("anon_phrased", "`anon text`__", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::AnonymousReference("anon text".to_string())
        );
    }
    #[test]
    fn test_handle_inline_match_anon_simple_variant() {
        let result = handle_inline_match("anon_simple", "anon_name__", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::AnonymousReference("anon_name".to_string())
        );
    }
    #[test]
    fn test_try_match_inline_multibyte_first_inner() {
        // Given
        let input = "*π*";
        let close_positions = find_valid_close_positions(input, 0, "*", false);
        // When
        let res = try_match_inline(input, 0, 1, &close_positions);
        // Then
        assert_eq!(res, Some((4, "π".to_string())));
    }
    #[test]
    fn test_try_match_inline_rejects_space_after_open_marker() {
        // Given: space immediately after marker is not valid markup
        let input = "* not emphasis *";
        let close_positions = find_valid_close_positions(input, 0, "*", false);
        // When
        let res = try_match_inline(input, 0, 1, &close_positions);
        // Then
        assert_eq!(res, None);
    }
    #[test]
    fn test_try_match_inline_requires_valid_end_boundary() {
        // Given: no valid end boundary
        let input = "*nospace*x";
        let close_positions = find_valid_close_positions(input, 0, "*", false);
        // When
        let res = try_match_inline(input, 0, 1, &close_positions);
        // Then
        assert_eq!(res, None);
    }
    #[test]
    fn test_find_valid_close_positions_finds_marker_with_valid_boundaries() {
        // Given: a lone valid "*" close candidate, not preceded by whitespace,
        // followed by whitespace
        let input = "*word* rest";
        // When
        let positions = find_valid_close_positions(input, 0, "*", false);
        // Then
        assert_eq!(positions, vec![5]);
    }
    #[test]
    fn test_find_valid_close_positions_rejects_marker_preceded_by_whitespace() {
        // Given: the marker is preceded by a space, so it can't close content
        let input = "*word * rest*";
        // When
        let positions = find_valid_close_positions(input, 0, "*", false);
        // Then: only the final "*" (preceded by "t") qualifies
        assert_eq!(positions, vec![12]);
    }
    #[test]
    fn test_find_valid_close_positions_rejects_marker_followed_by_disallowed_char() {
        // Given: the marker is immediately followed by a word character
        let input = "*word*x more *word* here";
        // When
        let positions = find_valid_close_positions(input, 0, "*", false);
        // Then: only the second "*word*" pair's closer qualifies
        assert_eq!(positions, vec![18]);
    }
    #[test]
    fn test_find_valid_close_positions_rejects_escaped_marker_unless_literal() {
        // Given: an escaped "*" shouldn't count as a valid closer for
        // emphasis, but escaping is irrelevant for inline literals
        let input = &escaped(r"word\* more");
        // When
        let emphasis_positions = find_valid_close_positions(input, 0, "*", false);
        let literal_positions = find_valid_close_positions(input, 0, "*", true);
        // Then
        assert_eq!(emphasis_positions, Vec::<usize>::new());
        assert_eq!(literal_positions, vec![5]);
    }
    #[test]
    fn test_find_valid_close_positions_respects_search_start() {
        // Given: two valid closers, one before and one at/after search_start
        let input = "*a* *b*";
        // When
        let positions = find_valid_close_positions(input, 4, "*", false);
        // Then: the closer at byte 2 (before search_start) is excluded
        assert_eq!(positions, vec![6]);
    }
}

#[cfg(test)]
mod pipeline_tests;
