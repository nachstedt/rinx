//! Converting rinx's source positions into the protocol's.
//!
//! The two disagree on purpose (ADR-003): a [`rinx_ast::Position`] is
//! 1-based in both components and counts columns in Unicode scalar values,
//! while an LSP position is 0-based and counts columns in whatever unit the
//! client and server agreed on — UTF-16 code units unless both say otherwise.
//! Neither side is "fixed" to match the other; this module is the one place
//! that translates, at the protocol boundary.

use lsp_types::{ClientCapabilities, PositionEncodingKind};
use rinx_ast::{Position, Span};

/// The column unit agreed with the client during `initialize`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionEncoding {
    /// UTF-16 code units: the protocol's default, and the only unit every
    /// client understands.
    Utf16,
    /// Unicode scalar values — exactly what a rinx column already counts, so
    /// no conversion is needed when a client offers it.
    Utf32,
}

impl PositionEncoding {
    /// The encoding to use with a client announcing `capabilities`.
    ///
    /// `utf-32` when the client lists it, since that is rinx's own unit;
    /// `utf-16` otherwise, which the protocol obliges every client to accept.
    #[must_use]
    pub fn negotiate(capabilities: &ClientCapabilities) -> Self {
        let offers_utf32 = capabilities
            .general
            .as_ref()
            .and_then(|general| general.position_encodings.as_ref())
            .is_some_and(|encodings| encodings.contains(&PositionEncodingKind::UTF32));
        if offers_utf32 {
            Self::Utf32
        } else {
            Self::Utf16
        }
    }

    /// The protocol's name for this encoding, as announced in the server's
    /// capabilities.
    #[must_use]
    pub fn kind(self) -> PositionEncodingKind {
        match self {
            Self::Utf16 => PositionEncodingKind::UTF16,
            Self::Utf32 => PositionEncodingKind::UTF32,
        }
    }

    /// How many units `c` occupies in this encoding.
    fn width(self, c: char) -> u32 {
        match self {
            // Everything beyond the Basic Multilingual Plane is a surrogate
            // pair; everything within it is one unit.
            Self::Utf16 => 1 + u32::from(c > '\u{FFFF}'),
            Self::Utf32 => 1,
        }
    }
}

/// The protocol position of `position` on a line whose text is `line_text`.
///
/// A column beyond the end of the line is clamped to the line's end, so a
/// diagnostic whose span ends just past the last character still lands on it.
#[must_use]
pub fn to_lsp_position(
    position: Position,
    line_text: &str,
    encoding: PositionEncoding,
) -> lsp_types::Position {
    let preceding = position.column.saturating_sub(1) as usize;
    let character = line_text
        .chars()
        .take(preceding)
        .map(|c| encoding.width(c))
        .sum();
    lsp_types::Position {
        line: position.line.saturating_sub(1),
        character,
    }
}

/// The protocol range of `span` within the document whose text is `text`.
///
/// A line past the end of the document is clamped to its last line, so a
/// span left over from a longer version of the text still points somewhere.
#[must_use]
pub fn to_lsp_range(span: Span, text: &str, encoding: PositionEncoding) -> lsp_types::Range {
    let lines: Vec<&str> = text.lines().collect();
    let convert = |position: Position| {
        let last_line = u32::try_from(lines.len()).unwrap_or(u32::MAX).max(1);
        let clamped = Position::new(position.line.clamp(1, last_line), position.column);
        let line_text = lines
            .get(clamped.line as usize - 1)
            .copied()
            .unwrap_or_default();
        to_lsp_position(clamped, line_text, encoding)
    };
    lsp_types::Range {
        start: convert(span.start),
        end: convert(span.end),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::GeneralClientCapabilities;

    fn capabilities_offering(encodings: Vec<PositionEncodingKind>) -> ClientCapabilities {
        ClientCapabilities {
            general: Some(GeneralClientCapabilities {
                position_encodings: Some(encodings),
                ..GeneralClientCapabilities::default()
            }),
            ..ClientCapabilities::default()
        }
    }

    #[test]
    fn test_negotiate_defaults_to_utf16() {
        // Given
        let capabilities = ClientCapabilities::default();

        // When
        let encoding = PositionEncoding::negotiate(&capabilities);

        // Then
        assert_eq!(encoding, PositionEncoding::Utf16);
    }

    #[test]
    fn test_negotiate_prefers_utf32_when_offered() {
        // Given
        let capabilities = capabilities_offering(vec![
            PositionEncodingKind::UTF16,
            PositionEncodingKind::UTF32,
        ]);

        // When
        let encoding = PositionEncoding::negotiate(&capabilities);

        // Then
        assert_eq!(encoding, PositionEncoding::Utf32);
    }

    #[test]
    fn test_negotiate_ignores_utf8() {
        // Given
        let capabilities = capabilities_offering(vec![PositionEncodingKind::UTF8]);

        // When
        let encoding = PositionEncoding::negotiate(&capabilities);

        // Then
        assert_eq!(encoding, PositionEncoding::Utf16);
    }

    #[test]
    fn test_kind_names_each_encoding() {
        // Given / When / Then
        assert_eq!(PositionEncoding::Utf16.kind(), PositionEncodingKind::UTF16);
        assert_eq!(PositionEncoding::Utf32.kind(), PositionEncodingKind::UTF32);
    }

    #[test]
    fn test_width_counts_utf16_units() {
        // Given / When / Then
        assert_eq!(PositionEncoding::Utf16.width('a'), 1);
        assert_eq!(PositionEncoding::Utf16.width('π'), 1);
        assert_eq!(PositionEncoding::Utf16.width('🦀'), 2);
        assert_eq!(PositionEncoding::Utf32.width('🦀'), 1);
    }

    #[test]
    fn test_to_lsp_position_makes_the_first_character_zero_zero() {
        // Given
        let position = Position::new(1, 1);

        // When
        let converted = to_lsp_position(position, "text", PositionEncoding::Utf16);

        // Then
        assert_eq!(converted, lsp_types::Position::new(0, 0));
    }

    #[test]
    fn test_to_lsp_position_counts_ascii_columns() {
        // Given — column 4 is the `f` of `.. foo::`
        let position = Position::new(3, 4);

        // When
        let converted = to_lsp_position(position, ".. foo::", PositionEncoding::Utf16);

        // Then
        assert_eq!(converted, lsp_types::Position::new(2, 3));
    }

    #[test]
    fn test_to_lsp_position_counts_bmp_characters_once() {
        // Given — `—` and `π` are one UTF-16 unit each
        let position = Position::new(1, 4);

        // When
        let converted = to_lsp_position(position, "—π x", PositionEncoding::Utf16);

        // Then
        assert_eq!(converted, lsp_types::Position::new(0, 3));
    }

    #[test]
    fn test_to_lsp_position_counts_astral_characters_twice_in_utf16() {
        // Given — the crab is a surrogate pair in UTF-16
        let position = Position::new(1, 3);

        // When
        let converted = to_lsp_position(position, "🦀 x", PositionEncoding::Utf16);

        // Then
        assert_eq!(converted, lsp_types::Position::new(0, 3));
    }

    #[test]
    fn test_to_lsp_position_passes_characters_through_in_utf32() {
        // Given
        let position = Position::new(1, 3);

        // When
        let converted = to_lsp_position(position, "🦀 x", PositionEncoding::Utf32);

        // Then
        assert_eq!(converted, lsp_types::Position::new(0, 2));
    }

    #[test]
    fn test_to_lsp_position_clamps_a_column_past_the_line_end() {
        // Given
        let position = Position::new(1, 40);

        // When
        let converted = to_lsp_position(position, "🦀x", PositionEncoding::Utf16);

        // Then
        assert_eq!(converted, lsp_types::Position::new(0, 3));
    }

    #[test]
    fn test_to_lsp_range_converts_both_ends_on_their_own_lines() {
        // Given
        let text = "Title\n🦀 .. foo::\n";
        let span = Span::new(Position::new(2, 3), Position::new(2, 11));

        // When
        let range = to_lsp_range(span, text, PositionEncoding::Utf16);

        // Then
        assert_eq!(
            range,
            lsp_types::Range::new(
                lsp_types::Position::new(1, 3),
                lsp_types::Position::new(1, 11)
            )
        );
    }

    #[test]
    fn test_to_lsp_range_clamps_a_line_past_the_document_end() {
        // Given
        let text = "one\ntwo\n";
        let span = Span::new(Position::new(5, 1), Position::new(5, 2));

        // When
        let range = to_lsp_range(span, text, PositionEncoding::Utf16);

        // Then
        assert_eq!(range.start, lsp_types::Position::new(1, 0));
        assert_eq!(range.end, lsp_types::Position::new(1, 1));
    }

    #[test]
    fn test_to_lsp_range_handles_an_empty_document() {
        // Given
        let span = Span::new(Position::new(1, 1), Position::new(1, 1));

        // When
        let range = to_lsp_range(span, "", PositionEncoding::Utf16);

        // Then
        assert_eq!(range, lsp_types::Range::default());
    }

    /// Properties over arbitrary text, against a reference that counts
    /// UTF-16 units with the standard library rather than by hand.
    mod properties {
        use super::*;
        use proptest::prelude::*;

        fn encodings() -> impl Strategy<Value = PositionEncoding> {
            prop_oneof![Just(PositionEncoding::Utf16), Just(PositionEncoding::Utf32)]
        }

        /// The width of the first `n` characters of `line`, counted the
        /// standard library's way.
        fn reference_width(line: &str, n: usize, encoding: PositionEncoding) -> u32 {
            let prefix: String = line.chars().take(n).collect();
            let units = match encoding {
                PositionEncoding::Utf16 => prefix.encode_utf16().count(),
                PositionEncoding::Utf32 => prefix.chars().count(),
            };
            u32::try_from(units).expect("a short line")
        }

        proptest! {
            #[test]
            fn test_to_lsp_position_agrees_with_the_standard_library(
                line in "\\PC{0,40}",
                column in 1u32..60,
                encoding in encodings(),
            ) {
                // Given
                let position = Position::new(7, column);

                // When
                let converted = to_lsp_position(position, &line, encoding);

                // Then
                prop_assert_eq!(converted.line, 6);
                prop_assert_eq!(
                    converted.character,
                    reference_width(&line, column as usize - 1, encoding)
                );
            }

            #[test]
            fn test_to_lsp_position_is_monotonic_and_stays_on_the_line(
                line in "\\PC{0,40}",
                column in 1u32..60,
                encoding in encodings(),
            ) {
                // Given
                let here = Position::new(1, column);
                let next = Position::new(1, column + 1);

                // When
                let at_here = to_lsp_position(here, &line, encoding);
                let at_next = to_lsp_position(next, &line, encoding);

                // Then
                prop_assert!(at_here.character <= at_next.character);
                prop_assert!(
                    at_next.character <= reference_width(&line, usize::MAX, encoding)
                );
            }

            #[test]
            fn test_to_lsp_range_lands_inside_the_document(
                lines in prop::collection::vec("[^\r\n]{0,20}", 0..6),
                start in (0u32..9, 0u32..30),
                end in (0u32..9, 0u32..30),
                encoding in encodings(),
            ) {
                // Given — any span, even one left over from a longer text
                let text = lines.join("\n");
                let span = Span::new(Position::new(start.0, start.1), Position::new(end.0, end.1));

                // When
                let range = to_lsp_range(span, &text, encoding);

                // Then
                for point in [range.start, range.end] {
                    let line = lines.get(point.line as usize).map_or("", String::as_str);
                    prop_assert!(point.line as usize <= lines.len().saturating_sub(1));
                    prop_assert!(point.character <= reference_width(line, usize::MAX, encoding));
                }
            }
        }
    }
}
