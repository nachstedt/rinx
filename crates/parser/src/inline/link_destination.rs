//! Reading a link destination: the text inside an embedded reference's
//! `<…>`, or after a hyperlink target's colon.
//!
//! docutils reads both the same way, which is why one function serves the
//! inline scan and the block-level target parser: text ending in an
//! *unescaped* underscore names another target (an alias, or indirect
//! target), and anything else is a URI with its whitespace removed — so a
//! long URL may wrap over lines, and a relative `page.html` or `#anchor` is a
//! URI like any other. Whether the underscore is escaped is only visible
//! before unescaping, which is why the decision is made here, once, and
//! recorded as a [`LinkDestination`] rather than left in a string for the
//! renderer to guess at.

use rinx_ast::{LinkDestination, TargetName};

use super::escapes::{EscapedText, is_escaped_at, unescape};

/// The destination `escaped` — text whose backslash escapes are already
/// markers, as the inline scan sees it — stands for.
pub(super) fn read_link_destination(escaped: &str) -> LinkDestination {
    let trimmed = escaped.trim();
    if let Some(name) = trimmed.strip_suffix('_')
        && !is_escaped_at(trimmed, name.len())
    {
        // A phrase name is written in backquotes after a target's colon:
        // `.. _Layout: `Layouts`_`.
        let name = name
            .strip_prefix('`')
            .and_then(|inner| inner.strip_suffix('`'))
            .unwrap_or(name);
        return LinkDestination::Name(TargetName::new(&unescape(name)));
    }
    let uri: String = trimmed.split_whitespace().collect();
    LinkDestination::Uri(unescape(&uri))
}

/// The destination written after a hyperlink target's colon, from its raw
/// source text — the form the block-level target parser holds.
pub(crate) fn read_target_destination(raw: &str) -> LinkDestination {
    read_link_destination(EscapedText::new(raw).as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_target_destination_reads_an_absolute_uri() {
        // When / Then
        assert_eq!(
            read_target_destination("https://python.org"),
            LinkDestination::Uri("https://python.org".to_string())
        );
    }

    #[test]
    fn test_read_target_destination_reads_a_relative_uri_and_a_fragment() {
        // When / Then — `configparser.rst` links `<#unnamed-sections>`.
        assert_eq!(
            read_target_destination("../howto/index.html"),
            LinkDestination::Uri("../howto/index.html".to_string())
        );
        assert_eq!(
            read_target_destination("#unnamed-sections"),
            LinkDestination::Uri("#unnamed-sections".to_string())
        );
    }

    #[test]
    fn test_read_target_destination_removes_whitespace_from_a_wrapped_uri() {
        // When / Then
        assert_eq!(
            read_target_destination("https://example.com/a/\n   very/long/path"),
            LinkDestination::Uri("https://example.com/a/very/long/path".to_string())
        );
    }

    #[test]
    fn test_read_target_destination_reads_an_alias() {
        // When / Then
        assert_eq!(
            read_target_destination("interp-isolation_"),
            LinkDestination::Name(TargetName::new("interp-isolation"))
        );
    }

    #[test]
    fn test_read_target_destination_reads_a_backquoted_phrase_alias() {
        // When / Then — `tkinter.ttk.rst`'s `.. _Layout: `Layouts`_`.
        assert_eq!(
            read_target_destination("`Layouts`_"),
            LinkDestination::Name(TargetName::new("Layouts"))
        );
    }

    #[test]
    fn test_read_target_destination_keeps_an_escaped_trailing_underscore_in_a_uri() {
        // When / Then
        assert_eq!(
            read_target_destination(r"https://example.com/name\_"),
            LinkDestination::Uri("https://example.com/name_".to_string())
        );
    }
}
