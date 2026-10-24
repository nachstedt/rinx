//! Which role's target the cursor is completing, read from its line alone.
//!
//! The parse is no help here: while the author types, the role is unfinished
//! — no closing backtick yet — so the parser has turned it into text. The
//! line is scanned instead, the way the inline parser reads it: a backslash
//! escapes the next character, ``` `` ``` opens an inline literal whose
//! backticks are not roles', and a single backtick opens interpreted text
//! that the next one closes. A role whose text wraps onto the next line is
//! not recognised; completion is offered while it is still on one.
//!
//! What the role *is* is not decided here: the role written before the
//! backtick, or the default role for a bare one, is the parser's to read.

use std::ops::Range;

/// The role a target is being typed for, as written before its backtick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WrittenRole {
    /// ``:name:`…` ``, with its domain and any `:external:` prefix:
    /// `ref`, `py:class`, `external+python:ref`.
    Named(String),
    /// A bare `` `…` ``, read as the default role.
    Default,
}

/// The role target the cursor is in, and what completing it replaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleContext {
    pub role: WrittenRole,
    /// The `~` and `.` written before the target, which shorten its display
    /// and change its search, and which a completion leaves in place.
    pub sigils: String,
    /// The target typed so far, after its sigils and up to the cursor.
    pub typed: String,
    /// The characters a completion replaces, as 0-based character indices on
    /// the line: the whole target after its sigils, including whatever
    /// follows the cursor up to the closing `` ` `` or `>`.
    pub replace: Range<usize>,
    /// Whether the target is the `<target>` of a `Title <target>`.
    pub angled: bool,
}

impl RoleContext {
    /// `line` with this target closed around `placeholder`, and everything
    /// after it dropped: the line a probe parses to learn what the parser
    /// reads the role as, where it is written.
    #[must_use]
    pub fn closed_line(&self, line: &str, placeholder: &str) -> String {
        let written: String = line.chars().take(self.replace.start).collect();
        let close = if self.angled { ">`" } else { "`" };
        format!("{written}{placeholder}{close}")
    }
}

/// The role target the cursor at character index `cursor` on `line` is in,
/// or `None` when it is in none.
///
/// In the `Title <target>` form only the target completes, once its `<` is
/// written. Nothing completes inside a `!`-prefixed target, which is never
/// looked up.
#[must_use]
pub fn role_at(line: &str, cursor: usize) -> Option<RoleContext> {
    let chars: Vec<char> = line.chars().collect();
    let cursor = cursor.min(chars.len());
    let OpenMarkup::Interpreted { backtick } = open_markup(&chars[..cursor]) else {
        return None;
    };
    let name = role_name_before(&chars[..backtick]);
    let role = if name.is_empty() {
        WrittenRole::Default
    } else {
        WrittenRole::Named(name)
    };
    let content_start = backtick + 1;
    let content = &chars[content_start..cursor];
    let (target_start, angled) = match content.iter().rposition(|&c| c == '<') {
        Some(at) if content[at..].contains(&'>') => return None,
        Some(at) => (content_start + at + 1, true),
        None => (content_start, false),
    };
    if chars.get(target_start) == Some(&'!') && target_start < cursor {
        return None;
    }
    let sigil_count = chars[target_start..cursor]
        .iter()
        .take(2)
        .take_while(|&&c| c == '~' || c == '.')
        .count();
    let sigils: String = chars[target_start..target_start + sigil_count]
        .iter()
        .collect();
    let typed_start = target_start + sigil_count;
    let typed: String = chars[typed_start..cursor].iter().collect();
    let rest = &chars[cursor..];
    let end = rest.iter().position(|&c| c == '`' || (angled && c == '>'));
    // The cursor is in a title whose `<target>` follows it: nothing to name.
    if !angled && rest[..end.unwrap_or(rest.len())].contains(&'<') {
        return None;
    }
    let end = end.map_or(cursor, |offset| cursor + offset);
    Some(RoleContext {
        role,
        sigils,
        typed,
        replace: typed_start..end,
        angled,
    })
}

/// Where the inline scan stands at the end of a line's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OpenMarkup {
    /// In plain text: every backtick is closed.
    Text,
    /// Inside interpreted text, opened by the backtick at this index.
    Interpreted { backtick: usize },
    /// Inside an inline literal, whose backticks are not roles'.
    Literal,
}

/// Where the inline scan stands at the end of `before`, read the way the
/// inline parser reads it: a backslash escapes the next character, ``` `` ```
/// opens an inline literal, and a single backtick opens interpreted text that
/// the next one closes.
pub(super) fn open_markup(before: &[char]) -> OpenMarkup {
    let mut open = None;
    let mut i = 0;
    while i < before.len() {
        match before[i] {
            '\\' => i += 1,
            '`' if open.is_some() => open = None,
            '`' if before.get(i + 1) == Some(&'`') => {
                let Some(closing) = (i + 2..before.len().saturating_sub(1))
                    .find(|&at| before[at] == '`' && before[at + 1] == '`')
                else {
                    return OpenMarkup::Literal;
                };
                i = closing + 1;
            }
            '`' => open = Some(i),
            _ => {}
        }
        i += 1;
    }
    open.map_or(OpenMarkup::Text, |backtick| OpenMarkup::Interpreted {
        backtick,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The context at the `|` in `marked`, which is removed first.
    fn at_marker(marked: &str) -> Option<RoleContext> {
        let cursor = marked.chars().position(|c| c == '|').expect("a marker");
        role_at(&marked.replacen('|', "", 1), cursor)
    }

    fn named(name: &str, typed: &str, replace: Range<usize>) -> RoleContext {
        RoleContext {
            role: WrittenRole::Named(name.to_string()),
            sigils: String::new(),
            typed: typed.to_string(),
            replace,
            angled: false,
        }
    }

    fn chars(text: &str) -> Vec<char> {
        text.chars().collect()
    }

    #[test]
    fn test_open_markup_is_text_once_every_backtick_closed() {
        // When / Then
        assert_eq!(
            open_markup(&chars("a :ref:`x` and ``y`` ")),
            OpenMarkup::Text
        );
    }

    #[test]
    fn test_open_markup_finds_the_backtick_opening_interpreted_text() {
        // When / Then
        assert_eq!(
            open_markup(&chars("a :ref:`x")),
            OpenMarkup::Interpreted { backtick: 7 }
        );
    }

    #[test]
    fn test_open_markup_is_inside_an_unclosed_literal() {
        // When / Then
        assert_eq!(open_markup(&chars("a ``x :ref:`")), OpenMarkup::Literal);
    }

    #[test]
    fn test_open_markup_skips_an_escaped_backtick() {
        // When / Then
        assert_eq!(open_markup(&chars("a \\`x")), OpenMarkup::Text);
    }

    #[test]
    fn test_role_at_opens_a_ref_at_its_backtick() {
        // When / Then
        assert_eq!(at_marker("See :ref:`|"), Some(named("ref", "", 10..10)));
    }

    #[test]
    fn test_role_at_reads_what_is_typed_and_replaces_up_to_the_closing_backtick() {
        // When / Then
        assert_eq!(
            at_marker(":ref:`ins|tall` and more"),
            Some(named("ref", "ins", 6..13))
        );
    }

    #[test]
    fn test_role_at_reads_any_role_with_its_domain_and_prefix() {
        // When / Then
        assert_eq!(
            at_marker(":py:class:`Pa|"),
            Some(named("py:class", "Pa", 11..13))
        );
        assert_eq!(
            at_marker(":external+python:ref:`|"),
            Some(named("external+python:ref", "", 22..22))
        );
    }

    #[test]
    fn test_role_at_reads_a_bare_backtick_as_the_default_role() {
        // When
        let context = at_marker("See `fo|").unwrap();

        // Then
        assert_eq!(context.role, WrittenRole::Default);
        assert_eq!(context.typed, "fo");
    }

    #[test]
    fn test_role_at_keeps_the_sigils_out_of_what_is_replaced() {
        // When
        let context = at_marker(":meth:`~.ru|`").unwrap();

        // Then
        assert_eq!(context.sigils, "~.");
        assert_eq!(context.typed, "ru");
        assert_eq!(context.replace, 9..11);
    }

    #[test]
    fn test_role_at_completes_the_target_of_a_titled_reference() {
        // When
        let context = at_marker(":ref:`Install it <ins|tall>`").unwrap();

        // Then
        assert_eq!(context.typed, "ins");
        assert_eq!(context.replace, 18..25);
        assert!(context.angled);
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
    }

    #[test]
    fn test_role_at_picks_the_last_role_on_the_line() {
        // When / Then
        assert_eq!(
            at_marker(":ref:`a` then :doc:`b|"),
            Some(named("doc", "b", 20..21))
        );
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
        assert_eq!(at_marker("``x`` :ref:`|"), Some(named("ref", "", 12..12)));
    }

    #[test]
    fn test_role_at_counts_characters_not_bytes() {
        // When / Then
        assert_eq!(at_marker("π :ref:`a|"), Some(named("ref", "a", 8..9)));
    }

    #[test]
    fn test_role_at_clamps_a_cursor_past_the_line_end() {
        // When / Then
        assert_eq!(role_at(":ref:`a", 40), Some(named("ref", "a", 6..7)));
    }

    #[test]
    fn test_closed_line_closes_the_target_around_the_placeholder() {
        // Given
        let line = "See :meth:`~.ru and more";
        let context = role_at(line, 15).unwrap();

        // When / Then
        assert_eq!(context.closed_line(line, "x"), "See :meth:`~.x`");
    }

    #[test]
    fn test_closed_line_closes_an_angled_target() {
        // Given
        let line = ":ref:`Title <ins";
        let context = role_at(line, 16).unwrap();

        // When / Then
        assert_eq!(context.closed_line(line, "x"), ":ref:`Title <x>`");
    }

    #[test]
    fn test_role_name_before_reads_the_name_between_colons() {
        // Given
        let before: Vec<char> = "see :std:doc:".chars().collect();

        // When / Then
        assert_eq!(role_name_before(&before), "std:doc");
        assert_eq!(role_name_before(&['x']), "");
    }
}
