//! RST backslash-escape processing, ported from docutils' two-phase model.
//!
//! docutils does not strip escapes where it finds them. It first rewrites every
//! `\X` to a NUL marker followed by `X` ([`EscapedText::new`], docutils'
//! `escape2null`), runs all inline-markup recognition over *that* string, and
//! only removes the markers when it emits text ([`unescape`], its `unescape`).
//! Two properties make the detour worth it:
//!
//! - An escaped markup character is no longer preceded by whitespace or an
//!   opener but by a NUL, so it simply fails the normal start-string test.
//!   Escape handling and markup recognition stop being separate problems.
//! - The rewrite replaces the backslash *byte* with a NUL *byte*, so it is
//!   byte-length preserving even when the escaped character is multi-byte.
//!   Every offset computed over the escaped text is equally valid over the
//!   source, which is why the surrounding parser needs no offset bookkeeping.
//!
//! Verbatim contexts (inline literals) use
//! [`unescape_keeping_backslashes`] instead, which turns the markers back into
//! backslashes — docutils' `restore_backslashes=True`. That is how a literal
//! keeps its backslashes without being excluded from the escaping pass.

/// The marker standing in for an escaping backslash. Chosen to match docutils,
/// which uses NUL for the same purpose.
pub(super) const MARKER: char = '\u{0}';

/// RST source whose backslash escapes have been replaced by [`MARKER`].
///
/// Constructible only through [`EscapedText::new`], so escaped text cannot be
/// conjured from an arbitrary string, and the markers can only be removed by
/// the two `unescape*` functions — which is what keeps a raw marker from
/// reaching an `InlineNode` and, from there, the rendered HTML.
pub(super) struct EscapedText(String);

impl EscapedText {
    /// Rewrites every `\X` to `MARKER X`, consuming the escaped character so a
    /// `\\` pair leaves one ordinary backslash behind. A trailing lone
    /// backslash becomes a bare marker, matching docutils.
    pub(super) fn new(raw: &str) -> Self {
        let mut escaped = String::with_capacity(raw.len());
        let mut chars = raw.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                escaped.push(MARKER);
                if let Some(escaped_char) = chars.next() {
                    escaped.push(escaped_char);
                }
            } else {
                escaped.push(c);
            }
        }
        Self(escaped)
    }

    /// The escaped form, for the markup-recognition machinery. Byte offsets
    /// into it are interchangeable with offsets into the source text.
    pub(super) fn as_str(&self) -> &str {
        &self.0
    }
}

/// Whether the character starting at `byte_pos` of already-escaped text is
/// escaped, i.e. directly preceded by a marker.
///
/// Replaces counting runs of backslashes: `\\*` escapes the backslash, not the
/// `*`, and that distinction already fell out of [`EscapedText::new`] consuming
/// each pair, so one lookback is now enough.
pub(super) fn is_escaped_at(escaped: &str, byte_pos: usize) -> bool {
    escaped[..byte_pos].ends_with(MARKER)
}

/// Removes the markers, yielding the text as it should be displayed.
///
/// A marker before a space or newline takes the whitespace with it — the RST
/// rule that makes `foo\ *bar*` join the emphasis to `foo` with no gap, and
/// that lets `\ ` stand for a deliberately empty table cell. Any other marker
/// simply drops, leaving the character it was protecting.
pub(super) fn unescape(escaped: &str) -> String {
    let mut out = String::with_capacity(escaped.len());
    let mut chars = escaped.chars();
    while let Some(c) = chars.next() {
        if c != MARKER {
            out.push(c);
            continue;
        }
        match chars.next() {
            // Escaped whitespace vanishes entirely, along with its marker, and
            // a trailing escape protects nothing — both leave no output.
            Some(' ' | '\n') | None => {}
            Some(protected) => out.push(protected),
        }
    }
    out
}

/// Turns the markers back into backslashes, for contexts that take their
/// content verbatim (inline literals). docutils' `restore_backslashes=True`.
pub(super) fn unescape_keeping_backslashes(escaped: &str) -> String {
    escaped.replace(MARKER, "\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spells out marker positions so the expectations below stay readable.
    fn m(text: &str) -> String {
        text.replace('@', "\u{0}")
    }

    // --- EscapedText::new ---

    #[test]
    fn test_new_replaces_an_escaping_backslash_with_a_marker() {
        // Given / When
        let escaped = EscapedText::new(r"a\*b");

        // Then
        assert_eq!(escaped.as_str(), m("a@*b"));
    }

    #[test]
    fn test_new_consumes_a_double_backslash_as_one_escape() {
        // Given a `\\` pair, which escapes the backslash itself
        // When
        let escaped = EscapedText::new(r"\\*");

        // Then — the surviving backslash is ordinary text, and the `*` is not
        // marked, so it stays a candidate marker for the recognizer to judge
        assert_eq!(escaped.as_str(), m(r"@\*"));
    }

    #[test]
    fn test_new_marks_a_trailing_lone_backslash() {
        // Given / When
        let escaped = EscapedText::new(r"trailing\");

        // Then
        assert_eq!(escaped.as_str(), m("trailing@"));
    }

    #[test]
    fn test_new_leaves_text_without_escapes_untouched() {
        // Given / When
        let escaped = EscapedText::new("plain text");

        // Then
        assert_eq!(escaped.as_str(), "plain text");
    }

    #[test]
    fn test_new_preserves_byte_length_for_multi_byte_escapes() {
        // Given an escaped multi-byte character — the property the whole
        // offset-sharing scheme rests on
        let raw = r"x\é y";

        // When
        let escaped = EscapedText::new(raw);

        // Then
        assert_eq!(escaped.as_str().len(), raw.len());
    }

    #[test]
    fn test_new_preserves_byte_length_across_representative_inputs() {
        // Given every escape shape the parser can meet
        for raw in [
            r"a\*b",
            r"\ ",
            r"\\*",
            r"x\é y",
            r"trailing\",
            r"\\",
            "plain",
        ] {
            // When
            let escaped = EscapedText::new(raw);

            // Then
            assert_eq!(
                escaped.as_str().len(),
                raw.len(),
                "byte length changed for {raw:?}"
            );
        }
    }

    // --- EscapedText::is_escaped_at ---

    #[test]
    fn test_is_escaped_at_reports_a_marked_character() {
        // Given `a\*b`, whose `*` sits at byte 2 of the escaped form
        let escaped = EscapedText::new(r"a\*b");

        // Then
        assert!(is_escaped_at(escaped.as_str(), 2));
    }

    #[test]
    fn test_is_escaped_at_reports_an_unmarked_character() {
        // Given the same text, whose `b` is not escaped
        let escaped = EscapedText::new(r"a\*b");

        // Then
        assert!(!is_escaped_at(escaped.as_str(), 3));
    }

    #[test]
    fn test_is_escaped_at_reports_the_first_character_as_unescaped() {
        // Given / When / Then — nothing precedes position 0
        assert!(!is_escaped_at(EscapedText::new(r"\*").as_str(), 0));
    }

    #[test]
    fn test_is_escaped_at_treats_an_escaped_backslash_as_not_escaping_the_next() {
        // Given `\\*`, where the backslash is escaped and the `*` is live
        let escaped = EscapedText::new(r"\\*");

        // Then — the `*` at byte 2 follows an ordinary backslash, not a marker
        assert!(!is_escaped_at(escaped.as_str(), 2));
    }

    // --- unescape ---

    #[test]
    fn test_unescape_drops_the_marker_and_keeps_the_protected_character() {
        // Given / When / Then
        assert_eq!(unescape(&m("a@*b")), "a*b");
    }

    #[test]
    fn test_unescape_removes_an_escaped_space_entirely() {
        // Given the escaped space RST uses to join markup to adjacent text,
        // and to mark a deliberately empty simple-table cell
        // When / Then
        assert_eq!(unescape(&m("@ ")), "");
    }

    #[test]
    fn test_unescape_removes_an_escaped_newline_entirely() {
        // Given / When / Then
        assert_eq!(unescape(&m("a@\nb")), "ab");
    }

    #[test]
    fn test_unescape_drops_a_trailing_marker() {
        // Given / When / Then
        assert_eq!(unescape(&m("trailing@")), "trailing");
    }

    #[test]
    fn test_unescape_keeps_an_escaped_backslash() {
        // Given / When / Then
        assert_eq!(unescape(&m(r"@\")), r"\");
    }

    #[test]
    fn test_unescape_leaves_unmarked_text_untouched() {
        // Given / When / Then
        assert_eq!(unescape("plain text"), "plain text");
    }

    // --- unescape_keeping_backslashes ---

    #[test]
    fn test_unescape_keeping_backslashes_restores_the_backslash() {
        // Given the content of an inline literal, which is verbatim
        // When / Then
        assert_eq!(
            unescape_keeping_backslashes(&m(r"some@\path")),
            r"some\\path"
        );
    }

    #[test]
    fn test_unescape_keeping_backslashes_restores_an_escaped_space() {
        // Given / When / Then — no whitespace rule applies in a verbatim context
        assert_eq!(unescape_keeping_backslashes(&m("@ ")), r"\ ");
    }

    // --- round trip ---

    #[test]
    fn test_escaping_then_restoring_backslashes_returns_the_source() {
        // Given every escape shape
        for raw in [r"a\*b", r"\ ", r"\\*", r"x\é y", r"\\", "plain"] {
            // When
            let restored = unescape_keeping_backslashes(EscapedText::new(raw).as_str());

            // Then
            assert_eq!(restored, raw, "round trip changed {raw:?}");
        }
    }

    // --- the cases verified against docutils 0.23 ---

    #[test]
    fn test_unescape_matches_docutils_on_the_verified_cases() {
        // Given each input and the output real docutils produces for it
        for (raw, expected) in [
            (r"a\*b", "a*b"),
            (r"\ ", ""),
            (r"\\*", r"\*"),
            (r"x\é y", "xé y"),
            (r"trailing\", "trailing"),
            (r"\\", r"\"),
            ("plain", "plain"),
            (r"spawn\*", "spawn*"),
            (r"Keep \*stars\* as is", "Keep *stars* as is"),
        ] {
            // When
            let actual = unescape(EscapedText::new(raw).as_str());

            // Then
            assert_eq!(actual, expected, "for input {raw:?}");
        }
    }
}
