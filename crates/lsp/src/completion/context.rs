//! Which reference role the cursor is completing, read from its line alone.
//!
//! The parse is no help here: while the author types, the role is unfinished
//! — no closing backtick yet — so the parser has turned it into text. The
//! line is scanned instead, the way the inline parser reads it: a backslash
//! escapes the next character, ``` `` ``` opens an inline literal whose
//! backticks are not roles', and a single backtick opens interpreted text
//! that the next one closes. A role whose text wraps onto the next line is
//! not recognised; completion is offered while it is still on one.

use std::ops::Range;

/// A reference role completion answers for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletedRole {
    /// `:ref:`, completed with labels.
    Ref,
    /// `:doc:` or `:std:doc:`, completed with document names.
    Doc,
}

/// The role the cursor is inside, and what completing it replaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleContext {
    pub role: CompletedRole,
    /// The target typed so far, from its start up to the cursor.
    pub typed: String,
    /// The characters a completion replaces, as 0-based character indices on
    /// the line: the whole target, including whatever follows the cursor up
    /// to the closing `` ` `` or `>`.
    pub replace: Range<usize>,
}

/// The reference role whose target the cursor at character index `cursor` on
/// `line` is in, or `None` when it is in no role completion answers for.
///
/// In the `Title <target>` form only the target completes, once its `<` is
/// written. Nothing completes inside a `!`-prefixed target, which is never
/// looked up, nor under an `:external:` prefix, since the server reads no
/// inventory.
#[must_use]
pub fn role_at(line: &str, cursor: usize) -> Option<RoleContext> {
    let chars: Vec<char> = line.chars().collect();
    let cursor = cursor.min(chars.len());
    let (role, content_start) = open_role_before(&chars[..cursor])?;
    let content = &chars[content_start..cursor];
    let (target_start, angled) = match content.iter().rposition(|&c| c == '<') {
        Some(at) if content[at..].contains(&'>') => return None,
        Some(at) => (content_start + at + 1, true),
        None => (content_start, false),
    };
    let typed: String = chars[target_start..cursor].iter().collect();
    if typed.starts_with('!') {
        return None;
    }
    let rest = &chars[cursor..];
    let end = rest.iter().position(|&c| c == '`' || (angled && c == '>'));
    // The cursor is in a title whose `<target>` follows it: nothing to name.
    if !angled && rest[..end.unwrap_or(rest.len())].contains(&'<') {
        return None;
    }
    let end = end.map_or(cursor, |offset| cursor + offset);
    Some(RoleContext {
        role,
        typed,
        replace: target_start..end,
    })
}

/// The role whose interpreted text is still open at the end of `before`, and
/// the index its content starts at; `None` when the text is closed, inside
/// an inline literal, or of a role completion does not answer for.
fn open_role_before(before: &[char]) -> Option<(CompletedRole, usize)> {
    let mut open: Option<(Option<CompletedRole>, usize)> = None;
    let mut i = 0;
    while i < before.len() {
        match before[i] {
            '\\' => i += 1,
            '`' if open.is_some() => open = None,
            '`' if before.get(i + 1) == Some(&'`') => {
                let closing = (i + 2..before.len().saturating_sub(1))
                    .find(|&at| before[at] == '`' && before[at + 1] == '`')?;
                i = closing + 1;
            }
            '`' => open = Some((completed_role(&role_name_before(&before[..i])), i + 1)),
            _ => {}
        }
        i += 1;
    }
    let (role, start) = open?;
    Some((role?, start))
}

/// The role name written immediately before a backtick at the end of
/// `before` — `ref` for ``:ref:` `` — or `""` when no `:name:` is there.
fn role_name_before(before: &[char]) -> String {
    let is_role_char = |c: char| c.is_alphanumeric() || "_.+-:".contains(c);
    let start = before
        .iter()
        .rposition(|&c| !is_role_char(c))
        .map_or(0, |at| at + 1);
    let spelled: String = before[start..].iter().collect();
    spelled
        .strip_prefix(':')
        .and_then(|name| name.strip_suffix(':'))
        .unwrap_or_default()
        .to_string()
}

/// The completed role a role named `name` is, as the parser spells them.
fn completed_role(name: &str) -> Option<CompletedRole> {
    match name {
        "ref" => Some(CompletedRole::Ref),
        "doc" | "std:doc" => Some(CompletedRole::Doc),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The context at the `|` in `marked`, which is removed first.
    fn at_marker(marked: &str) -> Option<RoleContext> {
        let cursor = marked.chars().position(|c| c == '|').expect("a marker");
        role_at(&marked.replacen('|', "", 1), cursor)
    }

    fn context(role: CompletedRole, typed: &str, replace: Range<usize>) -> RoleContext {
        RoleContext {
            role,
            typed: typed.to_string(),
            replace,
        }
    }

    #[test]
    fn test_role_at_opens_a_ref_at_its_backtick() {
        // When / Then
        assert_eq!(
            at_marker("See :ref:`|"),
            Some(context(CompletedRole::Ref, "", 10..10))
        );
    }

    #[test]
    fn test_role_at_reads_what_is_typed_and_replaces_up_to_the_closing_backtick() {
        // When / Then
        assert_eq!(
            at_marker(":ref:`ins|tall` and more"),
            Some(context(CompletedRole::Ref, "ins", 6..13))
        );
    }

    #[test]
    fn test_role_at_reads_both_spellings_of_doc() {
        // When / Then
        assert_eq!(
            at_marker(":doc:`gu|"),
            Some(context(CompletedRole::Doc, "gu", 6..8))
        );
        assert_eq!(
            at_marker(":std:doc:`/|"),
            Some(context(CompletedRole::Doc, "/", 10..11))
        );
    }

    #[test]
    fn test_role_at_completes_the_target_of_a_titled_reference() {
        // When / Then
        assert_eq!(
            at_marker(":ref:`Install it <ins|tall>`"),
            Some(context(CompletedRole::Ref, "ins", 18..25))
        );
    }

    #[test]
    fn test_role_at_offers_nothing_in_a_title() {
        // When / Then
        assert_eq!(at_marker(":ref:`Inst|all it <install>`"), None);
        assert_eq!(at_marker(":ref:`Install <install> |`"), None);
    }

    #[test]
    fn test_role_at_offers_nothing_after_the_role_closed() {
        // When / Then
        assert_eq!(at_marker(":ref:`install` and |"), None);
        assert_eq!(at_marker(":ref:`install` and `|"), None);
    }

    #[test]
    fn test_role_at_picks_the_last_role_on_the_line() {
        // When / Then
        assert_eq!(
            at_marker(":ref:`a` then :doc:`b|"),
            Some(context(CompletedRole::Doc, "b", 20..21))
        );
    }

    #[test]
    fn test_role_at_offers_nothing_for_other_roles() {
        // When / Then
        assert_eq!(at_marker(":numref:`|"), None);
        assert_eq!(at_marker(":py:class:`|"), None);
        assert_eq!(at_marker("`|"), None);
    }

    #[test]
    fn test_role_at_offers_nothing_under_an_external_prefix() {
        // When / Then
        assert_eq!(at_marker(":external:ref:`|"), None);
        assert_eq!(at_marker(":external+py:doc:`|"), None);
    }

    #[test]
    fn test_role_at_offers_nothing_for_a_target_never_looked_up() {
        // When / Then
        assert_eq!(at_marker(":ref:`!ins|"), None);
    }

    #[test]
    fn test_role_at_ignores_an_escaped_backtick() {
        // When / Then
        assert_eq!(at_marker(":ref:\\`|"), None);
    }

    #[test]
    fn test_role_at_skips_an_inline_literal() {
        // When / Then
        assert_eq!(at_marker("``:ref:`|"), None);
        assert_eq!(
            at_marker("``x`` :ref:`|"),
            Some(context(CompletedRole::Ref, "", 12..12))
        );
    }

    #[test]
    fn test_role_at_counts_characters_not_bytes() {
        // When / Then
        assert_eq!(
            at_marker("π :ref:`a|"),
            Some(context(CompletedRole::Ref, "a", 8..9))
        );
    }

    #[test]
    fn test_role_at_clamps_a_cursor_past_the_line_end() {
        // When / Then
        assert_eq!(
            role_at(":ref:`a", 40),
            Some(context(CompletedRole::Ref, "a", 6..7))
        );
    }

    #[test]
    fn test_open_role_before_ends_with_the_content_start() {
        // Given
        let before: Vec<char> = ":doc:`x".chars().collect();

        // When / Then
        assert_eq!(open_role_before(&before), Some((CompletedRole::Doc, 6)));
    }

    #[test]
    fn test_role_name_before_reads_the_name_between_colons() {
        // Given
        let before: Vec<char> = "see :std:doc:".chars().collect();

        // When / Then
        assert_eq!(role_name_before(&before), "std:doc");
        assert_eq!(role_name_before(&['x']), "");
    }

    #[test]
    fn test_completed_role_names_the_roles_the_parser_reads() {
        // When / Then
        assert_eq!(completed_role("ref"), Some(CompletedRole::Ref));
        assert_eq!(completed_role("doc"), Some(CompletedRole::Doc));
        assert_eq!(completed_role("std:doc"), Some(CompletedRole::Doc));
        // The parser reads no `:std:ref:` yet (#268).
        assert_eq!(completed_role("std:ref"), None);
    }
}
