//! Interpreted text written without a role before it: a bare `` `text` ``,
//! read as the default role, and `` `text`:role: ``, the role written after.
//!
//! Neither form gets a regex or a handler of its own. A role written in either
//! place is rewritten to the ``:role:`text` `` the role table already reads,
//! and built by the handler that spelling reaches — so `` `x` `` under
//! `.. default-role:: py:func` is exactly ``:py:func:`x` ``, and a default role
//! can be any role this build knows, including a schema's or one a
//! `.. role::` defined, without a list of its own to drift from that table.

use std::sync::LazyLock;

use regex::Regex;
use rinx_ast::{Domain, InlineNode, RoleRefusal};

use super::dispatch::handle_inline_match;
use super::escapes::{is_escaped_at, unescape};
use super::markup::{InlineMarkup, opening_search_start};
use super::punctuation::{can_follow_end_string, can_precede_start_string};
use super::regexes::SIMPLE_ROLE_REGEXES;
use super::roles::custom::handle_custom_role_match;
use super::roles::entity::handle_entity_role_match;
use super::roles::title_reference::title_reference_node;
use crate::context::ParseCtx;

/// docutils' `simplename`, the shape of a role name *it* reads: wider than
/// the names this build's roles have, so a role written as ``:a.b:`x` `` is
/// still recognized as a role — and left alone — rather than its text being
/// taken for bare interpreted text.
const SIMPLENAME: &str = r"[^\W_]+(?:[-._+:][^\W_]+)*";

/// A role written right after a closing backquote.
static SUFFIX_ROLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"^:(?P<role>{SIMPLENAME}):")).unwrap());

/// A role written right before a backquote, as ``:name:`text` `` writes one.
static PREFIX_ROLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r":{SIMPLENAME}:$")).unwrap());

/// What follows a closing backquote — see [`read_trailer`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Trailer {
    /// Nothing: the end-string context follows right away.
    None,
    /// `:role:`, the role written after the text.
    Role(String),
    /// `_` or `__`: the backquotes were a hyperlink reference's.
    Reference,
    /// A role and then `_` or `__`, which cannot both apply.
    RoleAndReference,
}

/// What follows a closing backquote at the start of `after`, and how many
/// bytes of it belong to the construct — or `None` when no reading of it ends
/// where docutils' end-string context allows.
///
/// Longest reading first, as docutils' pattern tries it: a role that is not
/// followed by the end-string context (``:sub:y``) is no role, and the
/// backquote may still close bare text, since a colon may follow one.
pub(super) fn read_trailer(after: &str) -> Option<(Trailer, usize)> {
    let mut readings = Vec::with_capacity(5);
    if let Some(caps) = SUFFIX_ROLE.captures(after) {
        let role_end = caps.get(0).map_or(0, |m| m.end());
        for underscores in (1..=leading_underscores(&after[role_end..])).rev() {
            readings.push((Trailer::RoleAndReference, role_end + underscores));
        }
        readings.push((Trailer::Role(caps["role"].to_string()), role_end));
    }
    for underscores in (1..=leading_underscores(after)).rev() {
        readings.push((Trailer::Reference, underscores));
    }
    readings.push((Trailer::None, 0));
    readings
        .into_iter()
        .find(|(_, length)| ends_markup(&after[*length..]))
}

/// How many of a reference suffix's underscores `text` starts with: two at
/// most, since `__` is the longest suffix there is.
fn leading_underscores(text: &str) -> usize {
    text.bytes().take(2).take_while(|&b| b == b'_').count()
}

/// Whether markup may end right before `rest`: at the end of the text, or
/// before a character docutils' end-string context allows.
fn ends_markup(rest: &str) -> bool {
    rest.chars().next().is_none_or(can_follow_end_string)
}

/// Every backquote at or after `search_start` that could close interpreted
/// text, with what follows it: not preceded by whitespace, not escaped, and
/// followed by something [`read_trailer`] accepts.
///
/// Computed once per scan, as [`super::markup`] does for its markers, so a
/// failed opener never rescans the text.
pub(super) fn find_interpreted_closes(
    full_text: &str,
    search_start: usize,
) -> Vec<(usize, Trailer, usize)> {
    full_text[search_start..]
        .match_indices('`')
        .map(|(offset, _)| search_start + offset)
        .filter(|&position| {
            full_text[..position]
                .chars()
                .next_back()
                .is_some_and(|c| !c.is_whitespace())
                && !is_escaped_at(full_text, position)
        })
        .filter_map(|position| {
            read_trailer(&full_text[position + 1..])
                .map(|(trailer, length)| (position, trailer, length))
        })
        .collect()
}

/// Interpreted text opening with the backquote at `start_pos`, as the end
/// byte offset and what was found, or `None` when it opens none.
///
/// None when a role is written right before the backquote — the role table
/// reads that, or it is a role this build does not know and stays text — or
/// when the first close ends a hyperlink reference, which the role table
/// reads too. A role together with a reference is refused here, so the text
/// is reported rather than half-read.
pub(super) fn try_match_interpreted(
    full_text: &str,
    start_pos: usize,
    closes: &[(usize, Trailer, usize)],
) -> Option<(usize, InlineMarkup)> {
    if follows_prefix_role(full_text, start_pos) {
        return None;
    }
    let search_pos = opening_search_start(full_text, start_pos, 1)?;
    let index = closes.partition_point(|(position, _, _)| *position < search_pos);
    let (close, trailer, length) = closes.get(index)?;
    let body = full_text[start_pos + 1..*close].to_string();
    let end = close + 1 + length;
    let markup = match trailer {
        Trailer::None => InlineMarkup::Interpreted {
            body,
            suffix_role: None,
        },
        Trailer::Role(role) => InlineMarkup::Interpreted {
            body,
            suffix_role: Some(role.clone()),
        },
        Trailer::Reference => return None,
        Trailer::RoleAndReference => InlineMarkup::Node(refused(
            &full_text[start_pos..end],
            RoleRefusal::RoleAndReference,
        )),
    };
    Some((end, markup))
}

/// Whether the backquote at `position` is the one after a ``:name:`` role,
/// itself written where markup may start.
fn follows_prefix_role(full_text: &str, position: usize) -> bool {
    PREFIX_ROLE
        .find(&full_text[..position])
        .is_some_and(|role| {
            full_text[..role.start()]
                .chars()
                .next_back()
                .is_none_or(can_precede_start_string)
        })
}

/// A role whose trailer the scan refused, as the source text it is shown as.
pub(super) fn refused(source: &str, refusal: RoleRefusal) -> InlineNode {
    InlineNode::RefusedRole {
        text: unescape(source),
        refusal,
        span: None,
    }
}

/// What a role written before its text is followed by, as the refusal and the
/// bytes of `after` it takes when it is a second role or a reference suffix:
/// docutils allows a role in one place only, and a role and a link not at
/// all.
pub(super) fn prefix_role_trailer(after: &str) -> Option<(RoleRefusal, usize)> {
    match read_trailer(after)? {
        (Trailer::None, _) => None,
        (Trailer::Role(_), length) => Some((RoleRefusal::MultipleRoles, length)),
        (Trailer::Reference | Trailer::RoleAndReference, length) => {
            Some((RoleRefusal::RoleAndReference, length))
        }
    }
}

/// Builds the node for interpreted text whose `body` was written between
/// backquotes, with `suffix_role` after them if one was. `source` is the
/// whole construct as written, which is what a role this build does not know
/// is left as — as for one written before the text.
pub(super) fn handle_interpreted_text(
    source: &str,
    body: &str,
    suffix_role: Option<&str>,
    default_domain: Domain,
    ctx: &ParseCtx<'_>,
) -> InlineNode {
    let default_role = ctx.default_role();
    let Some(role) = suffix_role.or_else(|| default_role.role_name()) else {
        return title_reference_node(body);
    };
    dispatch_as_role(role, body, default_domain, ctx)
        .unwrap_or_else(|| InlineNode::Text(source.to_string()))
}

/// The node ``:role:`body` `` would be, or `None` when no role answers to
/// that spelling — or when `body` holds something only the bare form could
/// (an escaped backquote), so the rewrite no longer reads as one role.
fn dispatch_as_role(
    role: &str,
    body: &str,
    default_domain: Domain,
    ctx: &ParseCtx<'_>,
) -> Option<InlineNode> {
    let spelled = spell_role(role, body);
    let kind = matching_role_kind(&spelled, ctx)?;
    Some(handle_inline_match(
        kind,
        &spelled,
        None,
        default_domain,
        ctx,
    ))
}

/// The `kind` tag of the role `name` is in the role table, or `None` when no
/// role answers to it: what makes a name a valid default role.
pub(crate) fn find_role_kind(name: &str, ctx: &ParseCtx<'_>) -> Option<&'static str> {
    matching_role_kind(&spell_role(name, "x"), ctx)
}

/// ``:role:`body` ``, the spelling the role table reads.
fn spell_role(role: &str, body: &str) -> String {
    format!(":{role}:`{body}`")
}

/// The first role table entry matching all of `spelled`, as the scan picks
/// among equally-placed matches. A name only the catch-all shape matched
/// counts only if the schema declares it or the document has defined it by
/// now: otherwise it would stay text, which is not a role.
fn matching_role_kind(spelled: &str, ctx: &ParseCtx<'_>) -> Option<&'static str> {
    let kind = SIMPLE_ROLE_REGEXES.iter().find_map(|(regex, kind)| {
        regex
            .find(spelled)
            .is_some_and(|m| m.start() == 0 && m.end() == spelled.len())
            .then_some(*kind)
    })?;
    let known = kind != "named_role"
        || handle_entity_role_match(spelled, ctx.schema).is_some()
        || handle_custom_role_match(spelled, ctx.document_roles()).is_some();
    known.then_some(kind)
}

#[cfg(test)]
mod tests {
    use rinx_ast::{ObjectType, PyObjectType, ScriptPosition};

    use super::*;
    use crate::default_role::DefaultRole;

    fn ctx() -> ParseCtx<'static> {
        ParseCtx::with_domain(Domain::Py)
    }

    #[test]
    fn test_read_trailer_reads_nothing_before_the_end_string_context() {
        // Given / When / Then
        assert_eq!(read_trailer(""), Some((Trailer::None, 0)));
        assert_eq!(read_trailer(" rest"), Some((Trailer::None, 0)));
        assert_eq!(read_trailer(", rest"), Some((Trailer::None, 0)));
    }

    #[test]
    fn test_read_trailer_reads_a_role() {
        // Given / When / Then
        assert_eq!(
            read_trailer(":sub: rest"),
            Some((Trailer::Role("sub".to_string()), 5))
        );
        assert_eq!(
            read_trailer(":py:func:"),
            Some((Trailer::Role("py:func".to_string()), 9))
        );
    }

    #[test]
    fn test_read_trailer_reads_both_reference_suffixes() {
        // Given / When / Then
        assert_eq!(read_trailer("_ rest"), Some((Trailer::Reference, 1)));
        assert_eq!(read_trailer("__."), Some((Trailer::Reference, 2)));
    }

    #[test]
    fn test_read_trailer_reads_a_role_and_a_reference() {
        // Given / When / Then
        assert_eq!(read_trailer(":sub:_"), Some((Trailer::RoleAndReference, 6)));
        assert_eq!(
            read_trailer(":sub:__ x"),
            Some((Trailer::RoleAndReference, 7))
        );
    }

    #[test]
    fn test_read_trailer_falls_back_to_a_shorter_reading() {
        // Given — a role followed by a letter is no role, but the colon may
        // still follow bare text
        // When / Then
        assert_eq!(read_trailer(":sub:y"), Some((Trailer::None, 0)));
    }

    #[test]
    fn test_read_trailer_refuses_what_no_reading_ends() {
        // Given / When / Then
        assert_eq!(read_trailer("x"), None);
        assert_eq!(read_trailer("___"), None);
        assert_eq!(read_trailer("_x"), None);
        assert_eq!(read_trailer("`"), None);
    }

    #[test]
    fn test_leading_underscores_counts_at_most_two() {
        // Given / When / Then
        assert_eq!(leading_underscores("x"), 0);
        assert_eq!(leading_underscores("_x"), 1);
        assert_eq!(leading_underscores("___"), 2);
    }

    #[test]
    fn test_ends_markup_at_the_end_or_before_allowed_characters() {
        // Given / When / Then
        assert!(ends_markup(""));
        assert!(ends_markup(" x"));
        assert!(ends_markup(")"));
        assert!(!ends_markup("x"));
    }

    #[test]
    fn test_find_interpreted_closes_skips_spaced_and_escaped_backquotes() {
        // Given
        let text = crate::inline::escapes::EscapedText::new(r"`a ` \` b`:sub: c`x");

        // When
        let closes = find_interpreted_closes(text.as_str(), 1);

        // Then — the spaced and escaped ones close nothing, nor the last,
        // which a letter follows
        assert_eq!(closes, vec![(9, Trailer::Role("sub".to_string()), 5)]);
    }

    #[test]
    fn test_try_match_interpreted_reads_bare_text() {
        // Given
        let text = "a `b c` d";
        let closes = find_interpreted_closes(text, 0);

        // When
        let found = try_match_interpreted(text, 2, &closes);

        // Then
        assert_eq!(
            found,
            Some((
                7,
                InlineMarkup::Interpreted {
                    body: "b c".to_string(),
                    suffix_role: None
                }
            ))
        );
    }

    #[test]
    fn test_try_match_interpreted_reads_a_suffix_role() {
        // Given
        let text = "`2`:sup:";
        let closes = find_interpreted_closes(text, 0);

        // When
        let found = try_match_interpreted(text, 0, &closes);

        // Then
        assert_eq!(
            found,
            Some((
                8,
                InlineMarkup::Interpreted {
                    body: "2".to_string(),
                    suffix_role: Some("sup".to_string())
                }
            ))
        );
    }

    #[test]
    fn test_try_match_interpreted_leaves_a_reference_to_the_role_table() {
        // Given
        let text = "`link`_ and `x`";
        let closes = find_interpreted_closes(text, 0);

        // When / Then
        assert_eq!(try_match_interpreted(text, 0, &closes), None);
    }

    #[test]
    fn test_try_match_interpreted_refuses_a_role_and_a_reference() {
        // Given
        let text = "`x`:sub:_";
        let closes = find_interpreted_closes(text, 0);

        // When
        let found = try_match_interpreted(text, 0, &closes);

        // Then
        assert_eq!(
            found,
            Some((
                9,
                InlineMarkup::Node(InlineNode::RefusedRole {
                    text: "`x`:sub:_".to_string(),
                    refusal: RoleRefusal::RoleAndReference,
                    span: None,
                })
            ))
        );
    }

    #[test]
    fn test_try_match_interpreted_leaves_a_backquote_after_a_prefix_role() {
        // Given
        let text = ":a.b:`x`";
        let closes = find_interpreted_closes(text, 0);

        // When / Then
        assert_eq!(try_match_interpreted(text, 5, &closes), None);
    }

    #[test]
    fn test_follows_prefix_role_needs_the_role_where_markup_may_start() {
        // Given / When / Then
        assert!(follows_prefix_role(":ab:`", 4));
        assert!(follows_prefix_role("see :ab:`", 8));
        assert!(!follows_prefix_role("x:ab:`", 5));
        assert!(!follows_prefix_role("see:`", 4));
    }

    #[test]
    fn test_refused_unescapes_the_source_it_shows() {
        // Given
        let text = crate::inline::escapes::EscapedText::new(r":ref:`\*`_");

        // When
        let node = refused(text.as_str(), RoleRefusal::RoleAndReference);

        // Then
        assert_eq!(
            node,
            InlineNode::RefusedRole {
                text: ":ref:`*`_".to_string(),
                refusal: RoleRefusal::RoleAndReference,
                span: None,
            }
        );
    }

    #[test]
    fn test_prefix_role_trailer_refuses_a_second_role_or_a_reference() {
        // Given / When / Then
        assert_eq!(prefix_role_trailer(" x"), None);
        assert_eq!(
            prefix_role_trailer(":sup: x"),
            Some((RoleRefusal::MultipleRoles, 5))
        );
        assert_eq!(
            prefix_role_trailer("__"),
            Some((RoleRefusal::RoleAndReference, 2))
        );
        assert_eq!(
            prefix_role_trailer(":sup:_"),
            Some((RoleRefusal::RoleAndReference, 6))
        );
    }

    #[test]
    fn test_find_role_kind_knows_a_built_in_role_in_every_spelling() {
        // Given / When / Then
        assert_eq!(find_role_kind("func", &ctx()), Some("func"));
        assert_eq!(find_role_kind("py:func", &ctx()), Some("func"));
        assert_eq!(find_role_kind("t", &ctx()), Some("title-reference"));
        assert_eq!(find_role_kind("any", &ctx()), Some("any"));
    }

    #[test]
    fn test_find_role_kind_refuses_a_name_only_the_catch_all_matched() {
        // Given / When / Then
        assert_eq!(find_role_kind("nonsense", &ctx()), None);
    }

    #[test]
    fn test_find_role_kind_refuses_a_name_that_is_not_one() {
        // Given / When / Then
        assert_eq!(find_role_kind("", &ctx()), None);
        assert_eq!(find_role_kind("a b", &ctx()), None);
        assert_eq!(find_role_kind("func:`x` :ref", &ctx()), None);
    }

    #[test]
    fn test_spell_role_writes_the_prefix_form() {
        // Given / When / Then
        assert_eq!(spell_role("sub", "2"), ":sub:`2`");
    }

    #[test]
    fn test_matching_role_kind_prefers_the_first_table_entry() {
        // Given — `:sub:` also has the catch-all's shape
        // When / Then
        assert_eq!(matching_role_kind(":sub:`2`", &ctx()), Some("script"));
    }

    #[test]
    fn test_dispatch_as_role_builds_what_the_prefix_form_builds() {
        // Given / When
        let node = dispatch_as_role("sup", "2", Domain::Py, &ctx());

        // Then
        assert_eq!(
            node,
            Some(InlineNode::Script {
                position: ScriptPosition::Superscript,
                text: "2".to_string(),
                classes: Vec::new(),
            })
        );
    }

    #[test]
    fn test_handle_interpreted_text_reads_a_bare_text_as_a_title_by_default() {
        // Given / When
        let node = handle_interpreted_text("`Dune`", "Dune", None, Domain::Py, &ctx());

        // Then
        assert_eq!(node, InlineNode::TitleReference("Dune".to_string()));
    }

    #[test]
    fn test_handle_interpreted_text_applies_the_configured_default() {
        // Given
        let base = ctx();
        let role = DefaultRole::parse("func", &base).unwrap();
        let ctx = base.with_default_role(&role);

        // When
        let node = handle_interpreted_text("`f`", "f", None, Domain::Py, &ctx);

        // Then
        assert!(matches!(
            node,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Function),
                ref name,
                ..
            } if name == "f"
        ));
    }

    #[test]
    fn test_handle_interpreted_text_lets_a_suffix_role_win_over_the_default() {
        // Given / When
        let node = handle_interpreted_text("`2`:sub:", "2", Some("sub"), Domain::Py, &ctx());

        // Then
        assert!(matches!(node, InlineNode::Script { .. }), "{node:?}");
    }

    #[test]
    fn test_handle_interpreted_text_leaves_an_unknown_suffix_role_as_written() {
        // Given / When
        let node = handle_interpreted_text("`x`:nope:", "x", Some("nope"), Domain::Py, &ctx());

        // Then
        assert_eq!(node, InlineNode::Text("`x`:nope:".to_string()));
    }
}
