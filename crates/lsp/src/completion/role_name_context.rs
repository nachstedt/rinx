//! Which role name the cursor is typing, read from its line alone.
//!
//! A role being named is no role yet — no backtick follows it — so, as for a
//! role's target, the line is read instead of the parse: a name opens with a
//! `:` where inline markup may start (docutils' rule, the parser's
//! `can_precede_inline_markup`), outside every literal and interpreted text,
//! and runs over the characters a role name is written with, a domain's `:`
//! included (`:py:fu`).

use std::ops::Range;

use rinx_parser::can_precede_inline_markup;

use super::context::{OpenMarkup, open_markup};

/// A role name being typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleNameContext {
    /// The name typed so far, after its opening `:` and up to the cursor.
    pub typed: String,
    /// The characters a completion replaces, as 0-based character indices on
    /// the line: the whole name written, up to its closing `:`.
    pub replace: Range<usize>,
    /// Whether the closing `:` is already written after the name.
    pub closed: bool,
}

/// The role name the cursor at character index `cursor` on `line` is in, or
/// `None` when it is in none.
#[must_use]
pub fn role_name_at(line: &str, cursor: usize) -> Option<RoleNameContext> {
    let chars: Vec<char> = line.chars().collect();
    let cursor = cursor.min(chars.len());
    let opener = chars[..cursor]
        .iter()
        .rposition(|&c| !is_name_char(c) && c != ':')
        .map_or(0, |at| at + 1);
    if chars.get(opener) != Some(&':') || opener == cursor {
        return None;
    }
    if !opener
        .checked_sub(1)
        .is_none_or(|at| can_precede_inline_markup(chars[at]))
    {
        return None;
    }
    if open_markup(&chars[..opener]) != OpenMarkup::Text {
        return None;
    }
    let typed: String = chars[opener + 1..cursor].iter().collect();
    // A second colon right after the first is a literal block's `::` or a
    // field marker's empty name, never a role.
    if typed.starts_with(':') || typed.contains("::") {
        return None;
    }
    let end = cursor
        + chars[cursor..]
            .iter()
            .take_while(|&&c| is_name_char(c))
            .count();
    Some(RoleNameContext {
        typed,
        replace: opener + 1..end,
        closed: chars.get(end) == Some(&':'),
    })
}

/// Whether `c` may be part of a role name other than a domain's `:`: a
/// letter or digit, or one of docutils' joining characters.
fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || "_-.+".contains(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The context at the `|` in `marked`, which is removed first.
    fn at_marker(marked: &str) -> Option<RoleNameContext> {
        let cursor = marked.chars().position(|c| c == '|').expect("a marker");
        role_name_at(&marked.replacen('|', "", 1), cursor)
    }

    fn context(typed: &str, replace: Range<usize>, closed: bool) -> RoleNameContext {
        RoleNameContext {
            typed: typed.to_string(),
            replace,
            closed,
        }
    }

    #[test]
    fn test_role_name_at_opens_after_a_colon_in_prose() {
        // When / Then
        assert_eq!(at_marker("See :|"), Some(context("", 5..5, false)));
    }

    #[test]
    fn test_role_name_at_reads_a_domain_qualified_name() {
        // When / Then
        assert_eq!(
            at_marker("See :py:fu|"),
            Some(context("py:fu", 5..10, false))
        );
    }

    #[test]
    fn test_role_name_at_replaces_up_to_the_closing_colon() {
        // When / Then
        assert_eq!(at_marker("See :fu|nc:`x`"), Some(context("fu", 5..9, true)));
    }

    #[test]
    fn test_role_name_at_opens_at_the_start_of_a_line() {
        // When / Then
        assert_eq!(at_marker(":re|"), Some(context("re", 1..3, false)));
    }

    #[test]
    fn test_role_name_at_opens_after_an_opening_bracket() {
        // When / Then
        assert_eq!(at_marker("(:re|"), Some(context("re", 2..4, false)));
    }

    #[test]
    fn test_role_name_at_offers_nothing_inside_a_word() {
        // When / Then — `ratio:` and `1:2` are not role names
        assert_eq!(at_marker("ratio:|"), None);
        assert_eq!(at_marker("ratio 1:2|"), None);
    }

    #[test]
    fn test_role_name_at_offers_nothing_after_a_double_colon() {
        // When / Then
        assert_eq!(at_marker("Example ::|"), None);
        assert_eq!(at_marker(".. note::|"), None);
    }

    #[test]
    fn test_role_name_at_offers_nothing_inside_markup() {
        // When / Then
        assert_eq!(at_marker("A ``literal :re|"), None);
        assert_eq!(at_marker("A :ref:`target :re|"), None);
    }

    #[test]
    fn test_role_name_at_opens_after_closed_markup() {
        // When / Then
        assert_eq!(
            at_marker("A :ref:`x` and :d|"),
            Some(context("d", 16..17, false))
        );
    }
}
