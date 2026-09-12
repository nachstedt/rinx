//! Measuring a source line in *display columns* rather than bytes.
//!
//! Kept out of [`crate::headings`] (its only caller today) because measuring
//! text is a general-purpose operation, not part of the heading construct —
//! the same reason [`crate::indent`] exists.

use unicode_width::UnicodeWidthStr;

/// How many terminal columns `text` occupies — docutils' `column_width`.
///
/// This is what a section adornment's length must be compared against: an
/// author draws `=====` under what they *see*, so measuring the title in bytes
/// rejects every heading holding a non-ASCII character (see
/// [`crate::headings::detect_adornment`]).
///
/// Deliberately `unicode-width` rather than a transcription of docutils'
/// `east_asian_width` arithmetic. The two agree everywhere that matters; where
/// they differ — ZWJ emoji sequences, variation selectors, other
/// default-ignorable characters — this crate reports the *smaller* width,
/// which can only make an adornment long enough, never falsely too short. Do
/// not "correct" it toward docutils' count.
pub(crate) fn column_width(text: &str) -> usize {
    text.width()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_column_width_counts_ascii_as_one_column_each() {
        // Given an all-ASCII title
        let text = "Demo details";

        // When measuring it
        // Then every character is one column, matching its byte length
        assert_eq!(column_width(text), 12);
        assert_eq!(column_width(text), text.len());
    }

    #[test]
    fn test_column_width_counts_emoji_as_two_columns_not_four_bytes() {
        // Given a title prefixed with an emoji, as the sphinx-needs demo writes
        let text = "\u{1f50d} Demo details";

        // When measuring it
        // Then the emoji costs two columns, not the four bytes it encodes to
        assert_eq!(column_width(text), 15);
        assert_eq!(text.len(), 17);
    }

    #[test]
    fn test_column_width_counts_wide_cjk_as_two_columns() {
        // Given a CJK title
        let text = "\u{6f22}\u{5b57}";

        // When measuring it
        // Then each ideograph occupies two columns
        assert_eq!(column_width(text), 4);
    }

    #[test]
    fn test_column_width_ignores_combining_marks() {
        // Given a title whose accent is a combining mark rather than a
        // precomposed character
        let text = "e\u{301}te\u{301}";

        // When measuring it
        // Then the marks add no columns, though they add bytes
        assert_eq!(column_width(text), 3);
        assert_eq!(text.len(), 7);
    }

    #[test]
    fn test_column_width_of_empty_text_is_zero() {
        // Given no text
        // When measuring it
        // Then it occupies no columns
        assert_eq!(column_width(""), 0);
    }
}
