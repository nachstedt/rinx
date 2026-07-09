use crate::typography::apply_smart_typography;
use regex::Regex;
use rusty_sphinx_ast::{Domain, InlineNode, ObjectType};
use std::sync::LazyLock;

static REF_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":ref:`(?P<target>[^`]+)`").unwrap());
static PROGRAM_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":program:`(?P<name>[^`]+)`").unwrap());
static TERM_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":term:`(?P<content>[^`]+)`").unwrap());
static FUNC_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(?:(?P<domain>py|c):)?func:`(?P<name>[^`]+)`").unwrap());
static MOD_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(?:(?P<domain>py):)?mod:`(?P<name>[^`]+)`").unwrap());
static DATA_ROLE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r":(?:(?P<domain>py):)?(?P<role>data|const):`(?P<name>[^`]+)`").unwrap()
});
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

/// Parses a plain text string into a list of [`InlineNode`]s.
///
/// `default_domain` resolves any bare (unprefixed) domain role, e.g. `:func:`,
/// to a concrete [`Domain`] — mirroring how bare directives are resolved.
pub(super) fn parse_inline_text(paragraph_text: &str, default_domain: Domain) -> Vec<InlineNode> {
    let mut inlines = Vec::new();
    let mut last_match_end = 0;

    while last_match_end < paragraph_text.len() {
        let remaining = &paragraph_text[last_match_end..];

        let ref_match = REF_REGEX.find(remaining);
        let program_match = PROGRAM_ROLE_REGEX.find(remaining);
        let term_match = TERM_ROLE_REGEX.find(remaining);
        let func_match = FUNC_ROLE_REGEX.find(remaining);
        let mod_match = MOD_ROLE_REGEX.find(remaining);
        let data_match = DATA_ROLE_REGEX.find(remaining);
        let phrased_match = PHRASED_LINK_REGEX.find(remaining);
        let simple_match = SIMPLE_LINK_REGEX.find(remaining);
        let anon_phrased_match = ANONYMOUS_PHRASED_REGEX.find(remaining);
        let anon_simple_match = ANONYMOUS_SIMPLE_REGEX.find(remaining);
        let inline_markup_match = find_inline_markup(paragraph_text, last_match_end);

        let mut all_matches = Vec::new();
        if let Some(m) = ref_match {
            all_matches.push((m.start(), m.end(), "ref", None));
        }
        if let Some(m) = program_match {
            all_matches.push((m.start(), m.end(), "program", None));
        }
        if let Some(m) = term_match {
            all_matches.push((m.start(), m.end(), "term", None));
        }
        if let Some(m) = func_match {
            all_matches.push((m.start(), m.end(), "func", None));
        }
        if let Some(m) = mod_match {
            all_matches.push((m.start(), m.end(), "mod", None));
        }
        if let Some(m) = data_match {
            all_matches.push((m.start(), m.end(), "data", None));
        }
        if let Some(m) = anon_phrased_match {
            all_matches.push((m.start(), m.end(), "anon_phrased", None));
        }
        if let Some(m) = phrased_match {
            all_matches.push((m.start(), m.end(), "phrased", None));
        }
        if let Some(m) = anon_simple_match {
            all_matches.push((m.start(), m.end(), "anon_simple", None));
        }
        if let Some(m) = simple_match {
            all_matches.push((m.start(), m.end(), "simple", None));
        }
        if let Some((start, end, node)) = inline_markup_match {
            all_matches.push((start, end, "inline", Some(node)));
        }

        let earliest = all_matches
            .into_iter()
            .min_by_key(|(start, end, _, _)| (*start, std::cmp::Reverse(*end)));

        if let Some((start, end, kind, node_opt)) = earliest {
            if start > 0 {
                inlines.push(InlineNode::Text(apply_smart_typography(
                    &remaining[..start],
                )));
            }
            let m_str = &remaining[start..end];
            inlines.push(handle_inline_match(kind, m_str, node_opt, default_domain));
            last_match_end += end;
        } else {
            inlines.push(InlineNode::Text(apply_smart_typography(remaining)));
            break;
        }
    }
    inlines
}

/// The name/display/link-behavior of a domain-object role target, after
/// stripping an optional `!` (suppress link) or `~` (shorten display to the
/// last dotted component) prefix.
struct DomainObjectTarget {
    name: String,
    display: String,
    link: bool,
}

/// Parses a domain-object role's raw backtick-quoted target, resolving the
/// `!`/`~` prefix modifiers documented at
/// <https://www.sphinx-doc.org/en/master/usage/referencing.html>.
fn parse_domain_object_target(raw: &str) -> DomainObjectTarget {
    if let Some(name) = raw.strip_prefix('!') {
        DomainObjectTarget {
            name: name.to_string(),
            display: name.to_string(),
            link: false,
        }
    } else if let Some(name) = raw.strip_prefix('~') {
        let display = name.rsplit('.').next().unwrap_or(name).to_string();
        DomainObjectTarget {
            name: name.to_string(),
            display,
            link: true,
        }
    } else {
        DomainObjectTarget {
            name: raw.to_string(),
            display: raw.to_string(),
            link: true,
        }
    }
}

/// Builds the `InlineNode` for a matched `:func:`/`:py:func:`/`:c:func:` role.
fn handle_func_match(m_str: &str, default_domain: Domain) -> InlineNode {
    let caps = FUNC_ROLE_REGEX.captures(m_str).unwrap();
    let domain = caps
        .name("domain")
        .and_then(|m| m.as_str().parse::<Domain>().ok())
        .unwrap_or(default_domain);
    let target = parse_domain_object_target(&caps["name"]);
    // Both domains currently define "func", so this always resolves.
    let object_type =
        ObjectType::from_role_name(domain, "func").expect("every domain defines a 'func' role");
    InlineNode::DomainObjectReference {
        object_type,
        name: target.name,
        display: target.display,
        link: target.link,
    }
}

/// Builds the `InlineNode` for a matched `:mod:`/`:py:mod:` role, falling
/// back to plain text if the role doesn't resolve for the given domain
/// (`mod` is Python-only, so a bare role under a `c` default domain fails).
fn handle_mod_match(m_str: &str, default_domain: Domain) -> InlineNode {
    let caps = MOD_ROLE_REGEX.captures(m_str).unwrap();
    let domain = caps
        .name("domain")
        .and_then(|m| m.as_str().parse::<Domain>().ok())
        .unwrap_or(default_domain);
    match ObjectType::from_role_name(domain, "mod") {
        Some(object_type) => {
            let target = parse_domain_object_target(&caps["name"]);
            InlineNode::DomainObjectReference {
                object_type,
                name: target.name,
                display: target.display,
                link: target.link,
            }
        }
        None => InlineNode::Text(m_str.to_string()),
    }
}

/// Builds the `InlineNode` for a matched `:data:`/`:py:data:`/`:const:`/
/// `:py:const:` role, falling back to plain text if the role doesn't resolve
/// for the given domain (`data`/`const` are Python-only, like `mod`). Both
/// role spellings resolve to the same [`rusty_sphinx_ast::PyObjectType::Data`],
/// so `:data:` and `:const:` targeting the same name link to the same
/// `.. py:data::` definition.
fn handle_data_match(m_str: &str, default_domain: Domain) -> InlineNode {
    let caps = DATA_ROLE_REGEX.captures(m_str).unwrap();
    let domain = caps
        .name("domain")
        .and_then(|m| m.as_str().parse::<Domain>().ok())
        .unwrap_or(default_domain);
    match ObjectType::from_role_name(domain, &caps["role"]) {
        Some(object_type) => {
            let target = parse_domain_object_target(&caps["name"]);
            InlineNode::DomainObjectReference {
                object_type,
                name: target.name,
                display: target.display,
                link: target.link,
            }
        }
        None => InlineNode::Text(m_str.to_string()),
    }
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
            InlineNode::Reference(caps["target"].to_string())
        }
        "program" => {
            let caps = PROGRAM_ROLE_REGEX.captures(m_str).unwrap();
            InlineNode::Program(caps["name"].to_string())
        }
        "func" => handle_func_match(m_str, default_domain),
        "mod" => handle_mod_match(m_str, default_domain),
        "data" => handle_data_match(m_str, default_domain),
        "term" => {
            let caps = TERM_ROLE_REGEX.captures(m_str).unwrap();
            let content = &caps["content"];
            // Support :term:`display text <actual term>` syntax
            if let Some(angle_start) = content.rfind('<')
                && let Some(angle_end) = content[angle_start..].find('>')
            {
                let display = content[..angle_start].trim().to_string();
                let term = content[angle_start + 1..angle_start + angle_end]
                    .trim()
                    .to_string();
                return InlineNode::TermReference { display, term };
            }
            InlineNode::TermReference {
                display: content.to_string(),
                term: content.to_string(),
            }
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

        // Check character after end marker
        let after_end = abs_pos + marker_len;
        if after_end < full_text.len() {
            let next_char = full_text[after_end..].chars().next().unwrap();
            if !next_char.is_whitespace() && !"-.,:;!?\\/ '\" >)]}".contains(next_char) {
                search_pos = abs_pos + 1;
                continue;
            }
        }

        // Check for escaping of end marker (skip if is_literal)
        if !is_literal && abs_pos > 0 && full_text.as_bytes()[abs_pos - 1] == b'\\' {
            let mut bs_count = 0;
            let mut j = abs_pos - 1;
            while full_text.as_bytes()[j] == b'\\' {
                bs_count += 1;
                if j == 0 {
                    break;
                }
                j -= 1;
            }
            if bs_count % 2 != 0 {
                search_pos = abs_pos + 1;
                continue;
            }
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

        // Check for escaping
        if abs_i > 0 && full_text.as_bytes()[abs_i - 1] == b'\\' {
            // Count backslashes to see if it's escaped or the backslash itself is escaped
            let mut bs_count = 0;
            let mut j = abs_i - 1;
            while full_text.as_bytes()[j] == b'\\' {
                bs_count += 1;
                if j == 0 {
                    break;
                }
                j -= 1;
            }
            if bs_count % 2 != 0 {
                continue;
            }
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
        if !prev_char.is_whitespace() && !"-:/'\"<([{".contains(prev_char) {
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

    #[test]
    fn test_parse_domain_object_target_plain_name() {
        let target = parse_domain_object_target("foo");
        assert_eq!(target.name, "foo");
        assert_eq!(target.display, "foo");
        assert!(target.link);
    }

    #[test]
    fn test_parse_domain_object_target_bang_prefix_suppresses_link() {
        let target = parse_domain_object_target("!foo");
        assert_eq!(target.name, "foo");
        assert_eq!(target.display, "foo");
        assert!(!target.link);
    }

    #[test]
    fn test_parse_domain_object_target_tilde_prefix_shortens_dotted_name() {
        let target = parse_domain_object_target("~pkg.mod.foo");
        assert_eq!(target.name, "pkg.mod.foo");
        assert_eq!(target.display, "foo");
        assert!(target.link);
    }

    #[test]
    fn test_parse_domain_object_target_tilde_prefix_on_bare_name_is_a_no_op() {
        let target = parse_domain_object_target("~foo");
        assert_eq!(target.name, "foo");
        assert_eq!(target.display, "foo");
        assert!(target.link);
    }

    #[test]
    fn test_handle_func_match_resolves_via_given_domain() {
        let result = handle_func_match(":func:`foo`", Domain::C);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Function),
                name: "foo".to_string(),
                display: "foo".to_string(),
                link: true,
            }
        );
    }

    #[test]
    fn test_handle_mod_match_resolves_when_domain_defines_mod_role() {
        let result = handle_mod_match(":mod:`greetings`", Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Module),
                name: "greetings".to_string(),
                display: "greetings".to_string(),
                link: true,
            }
        );
    }

    #[test]
    fn test_handle_mod_match_falls_back_to_text_when_domain_lacks_mod_role() {
        let result = handle_mod_match(":mod:`greetings`", Domain::C);
        assert_eq!(result, InlineNode::Text(":mod:`greetings`".to_string()));
    }

    #[test]
    fn test_handle_data_match_resolves_data_role() {
        let result = handle_data_match(":data:`DEFAULT_TIMEOUT`", Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Data),
                name: "DEFAULT_TIMEOUT".to_string(),
                display: "DEFAULT_TIMEOUT".to_string(),
                link: true,
            }
        );
    }

    #[test]
    fn test_handle_data_match_resolves_const_role_to_same_object_type_as_data() {
        // Given — real Sphinx has no separate `py:const` directive; `:const:`
        // is just an alternate role spelling for a `py:data` object.
        let data_result = handle_data_match(":data:`DEFAULT_TIMEOUT`", Domain::Py);
        let const_result = handle_data_match(":const:`DEFAULT_TIMEOUT`", Domain::Py);

        // Then
        assert_eq!(data_result, const_result);
    }

    #[test]
    fn test_handle_data_match_falls_back_to_text_when_domain_lacks_data_role() {
        let result = handle_data_match(":data:`DEFAULT_TIMEOUT`", Domain::C);
        assert_eq!(
            result,
            InlineNode::Text(":data:`DEFAULT_TIMEOUT`".to_string())
        );
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
        assert_eq!(result, InlineNode::Reference("target".to_string()));
    }

    #[test]
    fn test_handle_inline_match_program_variant() {
        let result = handle_inline_match("program", ":program:`curl`", None, Domain::Py);
        assert_eq!(result, InlineNode::Program("curl".to_string()));
    }

    #[test]
    fn test_handle_inline_match_func_variant_bare_uses_default_domain() {
        let result = handle_inline_match("func", ":func:`foo`", None, Domain::C);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Function),
                name: "foo".to_string(),
                display: "foo".to_string(),
                link: true,
            }
        );
    }

    #[test]
    fn test_handle_inline_match_func_variant_explicit_py_domain() {
        let result = handle_inline_match("func", ":py:func:`foo`", None, Domain::C);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
                name: "foo".to_string(),
                display: "foo".to_string(),
                link: true,
            }
        );
    }

    #[test]
    fn test_handle_inline_match_func_variant_explicit_c_domain() {
        let result = handle_inline_match("func", ":c:func:`add`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::C(rusty_sphinx_ast::CObjectType::Function),
                name: "add".to_string(),
                display: "add".to_string(),
                link: true,
            }
        );
    }

    #[test]
    fn test_handle_inline_match_func_variant_bang_prefix_suppresses_link() {
        let result = handle_inline_match("func", ":func:`!foo`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
                name: "foo".to_string(),
                display: "foo".to_string(),
                link: false,
            }
        );
    }

    #[test]
    fn test_handle_inline_match_func_variant_tilde_prefix_shortens_display() {
        let result = handle_inline_match("func", ":func:`~pkg.mod.foo`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Function),
                name: "pkg.mod.foo".to_string(),
                display: "foo".to_string(),
                link: true,
            }
        );
    }

    #[test]
    fn test_handle_inline_match_mod_variant_bare_uses_default_domain() {
        let result = handle_inline_match("mod", ":mod:`greetings`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Module),
                name: "greetings".to_string(),
                display: "greetings".to_string(),
                link: true,
            }
        );
    }

    #[test]
    fn test_handle_inline_match_mod_variant_explicit_py_domain() {
        let result = handle_inline_match("mod", ":py:mod:`greetings`", None, Domain::C);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Module),
                name: "greetings".to_string(),
                display: "greetings".to_string(),
                link: true,
            }
        );
    }

    #[test]
    fn test_handle_inline_match_mod_variant_bang_prefix_suppresses_link() {
        let result = handle_inline_match("mod", ":mod:`!curses`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Module),
                name: "curses".to_string(),
                display: "curses".to_string(),
                link: false,
            }
        );
    }

    #[test]
    fn test_handle_inline_match_mod_variant_tilde_prefix_shortens_display() {
        let result = handle_inline_match("mod", ":mod:`~pkg.submodule`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Module),
                name: "pkg.submodule".to_string(),
                display: "submodule".to_string(),
                link: true,
            }
        );
    }

    #[test]
    fn test_handle_inline_match_mod_variant_falls_back_to_text_when_unresolvable() {
        // Given — the `mod` role is Python-only, so a bare `:mod:` role in a
        // library whose default domain is `c` doesn't resolve to any object type.
        let result = handle_inline_match("mod", ":mod:`greetings`", None, Domain::C);
        assert_eq!(result, InlineNode::Text(":mod:`greetings`".to_string()));
    }

    #[test]
    fn test_handle_inline_match_data_variant_bare_uses_default_domain() {
        let result = handle_inline_match("data", ":data:`DEFAULT_TIMEOUT`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Data),
                name: "DEFAULT_TIMEOUT".to_string(),
                display: "DEFAULT_TIMEOUT".to_string(),
                link: true,
            }
        );
    }

    #[test]
    fn test_handle_inline_match_data_variant_const_spelling_explicit_py_domain() {
        let result = handle_inline_match("data", ":py:const:`DEFAULT_TIMEOUT`", None, Domain::C);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Data),
                name: "DEFAULT_TIMEOUT".to_string(),
                display: "DEFAULT_TIMEOUT".to_string(),
                link: true,
            }
        );
    }

    #[test]
    fn test_handle_inline_match_data_variant_bang_prefix_suppresses_link() {
        let result = handle_inline_match("data", ":data:`!SECRET_KEY`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Data),
                name: "SECRET_KEY".to_string(),
                display: "SECRET_KEY".to_string(),
                link: false,
            }
        );
    }

    #[test]
    fn test_handle_inline_match_data_variant_tilde_prefix_shortens_display() {
        let result = handle_inline_match("data", ":data:`~pkg.CONST`", None, Domain::Py);
        assert_eq!(
            result,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(rusty_sphinx_ast::PyObjectType::Data),
                name: "pkg.CONST".to_string(),
                display: "CONST".to_string(),
                link: true,
            }
        );
    }

    #[test]
    fn test_handle_inline_match_data_variant_falls_back_to_text_when_unresolvable() {
        // Given — `data`/`const` roles are Python-only, so a bare `:const:`
        // role in a library whose default domain is `c` doesn't resolve.
        let result = handle_inline_match("data", ":const:`DEFAULT_TIMEOUT`", None, Domain::C);
        assert_eq!(
            result,
            InlineNode::Text(":const:`DEFAULT_TIMEOUT`".to_string())
        );
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
        let input = r"word\* more";
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
mod integration_tests {
    use crate::{parse, parse_with_domain};
    use rusty_sphinx_ast::Node;
    use rusty_sphinx_ast::{CObjectType, Domain, InlineNode, ObjectType, PyObjectType};

    #[test]
    fn test_parse_creates_inline_text_and_reference_nodes_for_paragraph() {
        let input = "Here is a :ref:`my-target` link.";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![
                InlineNode::Text("Here is a ".to_string()),
                InlineNode::Reference("my-target".to_string()),
                InlineNode::Text(" link.".to_string()),
            ])
        );
    }

    #[test]
    fn test_parse_creates_phrased_hyperlink_node() {
        let input = "Check the `Python Guide`_ for more.";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![
                InlineNode::Text("Check the ".to_string()),
                InlineNode::Hyperlink {
                    text: "Python Guide".to_string(),
                    target: "Python Guide".to_string()
                },
                InlineNode::Text(" for more.".to_string()),
            ])
        );
    }

    #[test]
    fn test_parse_creates_embedded_uri_hyperlink_node() {
        let input = "Check `Google <https://google.com>`_ now.";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![
                InlineNode::Text("Check ".to_string()),
                InlineNode::Hyperlink {
                    text: "Google".to_string(),
                    target: "https://google.com".to_string()
                },
                InlineNode::Text(" now.".to_string()),
            ])
        );
    }

    #[test]
    fn test_parse_creates_simple_link_node() {
        let input = "Refer to target_ for details.";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![
                InlineNode::Text("Refer to ".to_string()),
                InlineNode::Hyperlink {
                    text: "target".to_string(),
                    target: "target".to_string()
                },
                InlineNode::Text(" for details.".to_string()),
            ])
        );
    }

    #[test]
    fn test_parse_does_not_treat_snake_case_word_as_hyperlink() {
        // Given: a plain identifier with underscores but no trailing one
        let input = "Use my_variable_name in code.";
        // When
        let doc = parse("test.rst", input);
        // Then: the whole sentence stays a single Text node, no Hyperlink
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![InlineNode::Text(
                "Use my_variable_name in code.".to_string()
            )])
        );
    }

    #[test]
    fn test_parse_does_not_treat_multi_underscore_word_as_hyperlink() {
        // Given: several internal underscores but no trailing underscore
        let input = "call foo_bar_baz here";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![InlineNode::Text("call foo_bar_baz here".to_string())])
        );
    }

    #[test]
    fn test_parse_creates_simple_link_node_for_target_name_containing_underscore() {
        // Given: the target name itself contains an underscore, and ends
        // with the triggering trailing underscore
        let input = "See my_target_ here.";
        // When
        let doc = parse("test.rst", input);
        // Then: still recognized as a reference to "my_target"
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![
                InlineNode::Text("See ".to_string()),
                InlineNode::Hyperlink {
                    text: "my_target".to_string(),
                    target: "my_target".to_string()
                },
                InlineNode::Text(" here.".to_string()),
            ])
        );
    }

    #[test]
    fn test_parse_does_not_treat_word_with_underscores_as_anonymous_reference() {
        // Given: no trailing double underscore
        let input = "word_with_underscores stays text";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![InlineNode::Text(
                "word_with_underscores stays text".to_string()
            )])
        );
    }

    #[test]
    fn test_parse_creates_anonymous_target_node() {
        let input = ".. __: https://example.com\n\nText";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::AnonymousTarget {
                uri: "https://example.com".to_string()
            }
        );
    }

    #[test]
    fn test_parse_creates_anonymous_reference() {
        let input = "See `Example`__ and link__";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 4);
            assert_eq!(
                inlines[1],
                InlineNode::AnonymousReference("Example".to_string())
            );
            assert_eq!(
                inlines[3],
                InlineNode::AnonymousReference("link".to_string())
            );
        } else {
            panic!("Expected paragraph");
        }
    }

    #[test]
    fn test_parse_creates_anonymous_hyperlink_with_embedded_uri() {
        let input = "See `Google <https://google.com>`__";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 2);
            assert_eq!(
                inlines[1],
                InlineNode::AnonymousHyperlink {
                    text: "Google".to_string(),
                    target: "https://google.com".to_string()
                }
            );
        } else {
            panic!("Expected paragraph");
        }
    }

    #[test]
    fn test_parse_paragraph_with_emphasis() {
        let input = "*emphasized* text";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 2);
            assert_eq!(inlines[0], InlineNode::Emphasis("emphasized".to_string()));
            assert_eq!(inlines[1], InlineNode::Text(" text".to_string()));
        } else {
            panic!("Expected Paragraph node");
        }
    }

    #[test]
    fn test_parse_paragraph_with_strong_emphasis() {
        let input = "some **strong** text";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 3);
            assert_eq!(inlines[0], InlineNode::Text("some ".to_string()));
            assert_eq!(inlines[1], InlineNode::Strong("strong".to_string()));
            assert_eq!(inlines[2], InlineNode::Text(" text".to_string()));
        } else {
            panic!("Expected Paragraph node");
        }
    }

    #[test]
    fn test_parse_paragraph_with_inline_literal() {
        let input = "some ``venv`` text";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 3);
            assert_eq!(inlines[0], InlineNode::Text("some ".to_string()));
            assert_eq!(inlines[1], InlineNode::Literal("venv".to_string()));
            assert_eq!(inlines[2], InlineNode::Text(" text".to_string()));
        } else {
            panic!("Expected Paragraph node");
        }
    }

    #[test]
    fn test_parse_inline_literal_with_backslash() {
        let input = "``some\\path``";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 1);
            assert_eq!(inlines[0], InlineNode::Literal("some\\path".to_string()));
        } else {
            panic!("Expected Paragraph node");
        }
    }

    #[test]
    fn test_parse_inline_literal_ignoring_inner_markup() {
        let input = "``**bold**``";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 1);
            assert_eq!(inlines[0], InlineNode::Literal("**bold**".to_string()));
        } else {
            panic!("Expected Paragraph node");
        }
    }

    #[test]
    fn test_parse_paragraph_with_mixed_markup() {
        let input = "Go to *emphasis* or **strong** link.";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 5);
            assert_eq!(inlines[0], InlineNode::Text("Go to ".to_string()));
            assert_eq!(inlines[1], InlineNode::Emphasis("emphasis".to_string()));
            assert_eq!(inlines[2], InlineNode::Text(" or ".to_string()));
            assert_eq!(inlines[3], InlineNode::Strong("strong".to_string()));
            assert_eq!(inlines[4], InlineNode::Text(" link.".to_string()));
        } else {
            panic!("Expected Paragraph node");
        }
    }

    #[test]
    fn test_parse_paragraph_with_escaped_markup() {
        let input = r"Keep \*stars\* as is and **strong** text.";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 3);
            assert_eq!(
                inlines[0],
                InlineNode::Text(r"Keep \*stars\* as is and ".to_string())
            );
            assert_eq!(inlines[1], InlineNode::Strong("strong".to_string()));
            assert_eq!(inlines[2], InlineNode::Text(" text.".to_string()));
        } else {
            panic!("Expected Paragraph node");
        }
    }

    #[test]
    fn test_parse_paragraph_rejects_invalid_boundary_markup() {
        let input = "a*text* *text*b";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 1);
            assert_eq!(inlines[0], InlineNode::Text("a*text* *text*b".to_string()));
        } else {
            panic!("Expected Paragraph node");
        }
    }

    #[test]
    fn test_parse_paragraph_with_many_unclosed_emphasis_markers_stays_fast() {
        // Given: many isolated, never-validly-closed single-star tokens (each
        // "*" is followed by a space before the next one, so none can close
        // any other) — the exact pathological shape from rust_review.md
        // finding #3, which previously made find_inline_markup/
        // try_match_inline rescan the remaining text from every failed
        // candidate, causing O(n^2) parse time.
        let input = "*word ".repeat(20_000);

        // When
        let start = std::time::Instant::now();
        let doc = parse("test.rst", &input);
        let elapsed = start.elapsed();

        // Then: stays well under a second (the O(n^2) implementation took
        // several seconds at this size); still parses as a single
        // unmatched-markup Text node.
        assert!(
            elapsed.as_secs() < 3,
            "parsing took too long: {elapsed:?} (quadratic regression?)"
        );
        assert_eq!(doc.nodes.len(), 1);
    }

    #[test]
    fn test_parse_paragraph_with_punctuation_boundaries() {
        let input = "(*emphasis*), [**strong**];";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 5);
            assert_eq!(inlines[0], InlineNode::Text("(".to_string()));
            assert_eq!(inlines[1], InlineNode::Emphasis("emphasis".to_string()));
            assert_eq!(inlines[2], InlineNode::Text("), [".to_string()));
            assert_eq!(inlines[3], InlineNode::Strong("strong".to_string()));
            assert_eq!(inlines[4], InlineNode::Text("];".to_string()));
        } else {
            panic!("Expected Paragraph node");
        }
    }

    #[test]
    fn test_parse_paragraph_with_emphasis_multibyte() {
        let input = "*\u{03c0}* text";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 2);
            assert_eq!(inlines[0], InlineNode::Emphasis("\u{03c0}".to_string()));
            assert_eq!(inlines[1], InlineNode::Text(" text".to_string()));
        } else {
            panic!("Expected Paragraph node");
        }
    }

    #[test]
    fn test_parse_paragraph_with_program_role() {
        let input = "Run :program:`curl` to download files.";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 3);
            assert_eq!(inlines[0], InlineNode::Text("Run ".to_string()));
            assert_eq!(inlines[1], InlineNode::Program("curl".to_string()));
            assert_eq!(
                inlines[2],
                InlineNode::Text(" to download files.".to_string())
            );
        } else {
            panic!("Expected Paragraph node");
        }
    }

    #[test]
    fn test_parse_paragraph_with_multiple_program_roles_and_punctuation() {
        let input = "Use :program:`git` or :program:`hg` to manage code.";
        let doc = parse("test.rst", input);
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(inlines.len(), 5);
            assert_eq!(inlines[0], InlineNode::Text("Use ".to_string()));
            assert_eq!(inlines[1], InlineNode::Program("git".to_string()));
            assert_eq!(inlines[2], InlineNode::Text(" or ".to_string()));
            assert_eq!(inlines[3], InlineNode::Program("hg".to_string()));
            assert_eq!(inlines[4], InlineNode::Text(" to manage code.".to_string()));
        } else {
            panic!("Expected Paragraph node");
        }
    }

    #[test]
    fn test_parse_term_role_basic() {
        let input = "See :term:`environment` for details.\n";
        let doc = parse("test.rst", input);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            let term_ref = inlines
                .iter()
                .find(|n| matches!(n, InlineNode::TermReference { .. }));
            assert!(term_ref.is_some(), "Expected TermReference in paragraph");
            if let Some(InlineNode::TermReference { display, term }) = term_ref {
                assert_eq!(display, "environment");
                assert_eq!(term, "environment");
            }
        } else {
            panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_term_role_with_display_text() {
        let input = "See :term:`the env <environment>` here.\n";
        let doc = parse("test.rst", input);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            let term_ref = inlines
                .iter()
                .find(|n| matches!(n, InlineNode::TermReference { .. }));
            assert!(term_ref.is_some());
            if let Some(InlineNode::TermReference { display, term }) = term_ref {
                assert_eq!(display, "the env");
                assert_eq!(term, "environment");
            }
        } else {
            panic!("Expected Paragraph");
        }
    }

    #[test]
    fn test_parse_term_role_mixed_with_surrounding_text() {
        let input = "Before :term:`foo` after.\n";
        let doc = parse("test.rst", input);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert!(inlines.len() >= 3, "Expected text + term + text");
            assert!(matches!(&inlines[0], InlineNode::Text(t) if t == "Before "));
            assert!(matches!(&inlines[1], InlineNode::TermReference { term, .. } if term == "foo"));
        } else {
            panic!("Expected Paragraph");
        }
    }

    #[test]
    fn test_parse_bare_func_role_resolves_via_default_domain() {
        // Given
        let input = "See :func:`greet` for details.";

        // When
        let doc = parse_with_domain("test.rst", input, Domain::C);

        // Then
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(
                inlines[1],
                InlineNode::DomainObjectReference {
                    object_type: ObjectType::C(CObjectType::Function),
                    name: "greet".to_string(),
                    display: "greet".to_string(),
                    link: true,
                }
            );
        } else {
            panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_prefixed_func_role_ignores_default_domain() {
        // Given
        let input = "See :py:func:`greet` and :c:func:`add`.";

        // When
        let doc = parse_with_domain("test.rst", input, Domain::C);

        // Then
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert!(inlines.contains(&InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Function),
                name: "greet".to_string(),
                display: "greet".to_string(),
                link: true,
            }));
            assert!(inlines.contains(&InlineNode::DomainObjectReference {
                object_type: ObjectType::C(CObjectType::Function),
                name: "add".to_string(),
                display: "add".to_string(),
                link: true,
            }));
        } else {
            panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_bare_mod_role_resolves_via_default_domain() {
        // Given
        let input = "See :mod:`greetings` for details.";

        // When
        let doc = parse_with_domain("test.rst", input, Domain::Py);

        // Then
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(
                inlines[1],
                InlineNode::DomainObjectReference {
                    object_type: ObjectType::Py(PyObjectType::Module),
                    name: "greetings".to_string(),
                    display: "greetings".to_string(),
                    link: true,
                }
            );
        } else {
            panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_prefixed_mod_role_ignores_default_domain() {
        // Given
        let input = "See :py:mod:`greetings`.";

        // When
        let doc = parse_with_domain("test.rst", input, Domain::C);

        // Then
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert!(inlines.contains(&InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Module),
                name: "greetings".to_string(),
                display: "greetings".to_string(),
                link: true,
            }));
        } else {
            panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_mod_role_with_bang_prefix_suppresses_link_end_to_end() {
        // Given
        let input = "See :mod:`!curses` for details.";

        // When
        let doc = parse_with_domain("test.rst", input, Domain::Py);

        // Then
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert!(inlines.contains(&InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Module),
                name: "curses".to_string(),
                display: "curses".to_string(),
                link: false,
            }));
        } else {
            panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_bare_data_role_resolves_via_default_domain() {
        // Given
        let input = "See :data:`DEFAULT_TIMEOUT` for details.";

        // When
        let doc = parse_with_domain("test.rst", input, Domain::Py);

        // Then
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert_eq!(
                inlines[1],
                InlineNode::DomainObjectReference {
                    object_type: ObjectType::Py(PyObjectType::Data),
                    name: "DEFAULT_TIMEOUT".to_string(),
                    display: "DEFAULT_TIMEOUT".to_string(),
                    link: true,
                }
            );
        } else {
            panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_prefixed_const_role_ignores_default_domain_and_matches_data_target() {
        // Given — `:py:const:` referencing the same name as a `.. py:data::`
        // definition must resolve to the identical `DomainObjectReference` a
        // `:py:data:` role would produce, since both share one namespace.
        let input = "See :py:const:`DEFAULT_TIMEOUT`.";

        // When
        let doc = parse_with_domain("test.rst", input, Domain::C);

        // Then
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert!(inlines.contains(&InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Data),
                name: "DEFAULT_TIMEOUT".to_string(),
                display: "DEFAULT_TIMEOUT".to_string(),
                link: true,
            }));
        } else {
            panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_data_role_with_bang_prefix_suppresses_link_end_to_end() {
        // Given
        let input = "See :data:`!SECRET_KEY` for details.";

        // When
        let doc = parse_with_domain("test.rst", input, Domain::Py);

        // Then
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert!(inlines.contains(&InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Data),
                name: "SECRET_KEY".to_string(),
                display: "SECRET_KEY".to_string(),
                link: false,
            }));
        } else {
            panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_func_role_with_tilde_prefix_shortens_display_end_to_end() {
        // Given
        let input = "See :func:`~greetings.shout` for details.";

        // When
        let doc = parse_with_domain("test.rst", input, Domain::Py);

        // Then
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert!(inlines.contains(&InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Function),
                name: "greetings.shout".to_string(),
                display: "shout".to_string(),
                link: true,
            }));
        } else {
            panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_mixed_term_and_func_roles_in_one_paragraph() {
        // Given
        let input = "The :term:`environment` affects :func:`greet`.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert!(
                inlines
                    .iter()
                    .any(|n| matches!(n, InlineNode::TermReference { .. }))
            );
            assert!(
                inlines
                    .iter()
                    .any(|n| matches!(n, InlineNode::DomainObjectReference { .. }))
            );
        } else {
            panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_paragraph_converts_triple_hyphen_to_em_dash() {
        // Given
        let input = "wait---no, that's wrong.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![InlineNode::Text(
                "wait\u{2014}no, that's wrong.".to_string()
            )])
        );
    }

    #[test]
    fn test_parse_paragraph_converts_double_hyphen_to_en_dash() {
        // Given
        let input = "See pages 10--20.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![InlineNode::Text(
                "See pages 10\u{2013}20.".to_string()
            )])
        );
    }

    #[test]
    fn test_parse_paragraph_converts_triple_dot_to_ellipsis() {
        // Given
        let input = "Wait... what happened?";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![InlineNode::Text(
                "Wait\u{2026} what happened?".to_string()
            )])
        );
    }

    #[test]
    fn test_parse_paragraph_applies_smart_typography_inside_strong_emphasis() {
        // Given
        let input = "This is **really---important**.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert!(inlines.contains(&InlineNode::Strong("really\u{2014}important".to_string())));
        } else {
            panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_paragraph_does_not_apply_smart_typography_inside_inline_literal() {
        // Given — literal/code content must not be transformed
        let input = "Run ``git log a---b``.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            assert!(inlines.contains(&InlineNode::Literal("git log a---b".to_string())));
        } else {
            panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
        }
    }
}
