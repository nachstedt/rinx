use serde::{Deserialize, Serialize};

/// The enumeration sequence an enumerated list counts in.
///
/// These are docutils' five `enumtype`s. The variant order is significant:
/// [`EnumeratorSequence::RESOLUTION_ORDER`] resolves an ambiguous enumerator text
/// (`v` is both a valid lower-alpha letter and the roman numeral 5) by taking
/// the first sequence that matches, exactly as docutils' `parse_enumerator`
/// iterates `self.enum.sequences`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnumeratorSequence {
    /// `1`, `2`, `3`, …
    Arabic,
    /// `a`, `b`, `c`, … `z`
    LowerAlpha,
    /// `A`, `B`, `C`, … `Z`
    UpperAlpha,
    /// `i`, `ii`, `iii`, …
    LowerRoman,
    /// `I`, `II`, `III`, …
    UpperRoman,
}

/// The largest ordinal a roman numeral can express. docutils' `roman` module
/// raises above this, which makes such an enumerator "not a list item".
const MAX_ROMAN: u32 = 4999;

/// The largest ordinal a single-letter alphabetic enumerator can express.
/// docutils' `make_enumerator` returns `None` past `z`.
const MAX_ALPHA: u32 = 26;

impl EnumeratorSequence {
    /// The order an ambiguous enumerator text is resolved in, matching
    /// docutils' `self.enum.sequences`. Arabic first, then alpha, then roman —
    /// which is why a bare `v.` starts a *lower-alpha* list, not a roman one.
    pub const RESOLUTION_ORDER: [Self; 5] = [
        Self::Arabic,
        Self::LowerAlpha,
        Self::UpperAlpha,
        Self::LowerRoman,
        Self::UpperRoman,
    ];

    /// Whether `text` is shaped like a member of this sequence.
    ///
    /// This is the character-class test only (docutils' `sequenceregexps`); it
    /// says nothing about whether the text denotes a valid ordinal, which is
    /// what [`Self::ordinal_of`] answers. `mmmmm` matches `LowerRoman`'s shape
    /// but has no ordinal.
    #[must_use]
    pub fn matches(self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        match self {
            Self::Arabic => text.chars().all(|c| c.is_ascii_digit()),
            Self::LowerAlpha => {
                text.len() == 1 && text.starts_with(|c: char| c.is_ascii_lowercase())
            }
            Self::UpperAlpha => {
                text.len() == 1 && text.starts_with(|c: char| c.is_ascii_uppercase())
            }
            Self::LowerRoman => text.chars().all(|c| "ivxlcdm".contains(c)),
            Self::UpperRoman => text.chars().all(|c| "IVXLCDM".contains(c)),
        }
    }

    /// Converts an enumerator text to its ordinal, or `None` when the text is
    /// not a well-formed member of this sequence.
    ///
    /// Roman numerals must be in canonical form: `iiii` has no ordinal, `iv`
    /// does. This mirrors docutils' `roman.fromRoman`, which rejects
    /// non-canonical spellings rather than summing them.
    #[must_use]
    pub fn ordinal_of(self, text: &str) -> Option<u32> {
        if !self.matches(text) {
            return None;
        }
        match self {
            Self::Arabic => text.parse().ok(),
            Self::LowerAlpha => alpha_ordinal(text, 'a'),
            Self::UpperAlpha => alpha_ordinal(text, 'A'),
            Self::LowerRoman => from_roman(&text.to_ascii_uppercase()),
            Self::UpperRoman => from_roman(text),
        }
    }

    /// Renders `ordinal` as this sequence's enumerator text, or `None` when the
    /// ordinal is outside the sequence's range (past `z`, or past roman 4999).
    #[must_use]
    pub fn render(self, ordinal: u32) -> Option<String> {
        match self {
            Self::Arabic => Some(ordinal.to_string()),
            Self::LowerAlpha => alpha_text(ordinal, 'a'),
            Self::UpperAlpha => alpha_text(ordinal, 'A'),
            Self::LowerRoman => to_roman(ordinal).map(|s| s.to_ascii_lowercase()),
            Self::UpperRoman => to_roman(ordinal),
        }
    }

    /// Whether `ordinal` is expressible in this sequence. Arabic is unbounded
    /// (and admits `0`, which docutils accepts from a literal `0.`); the others
    /// have hard ceilings.
    #[must_use]
    pub fn accepts_ordinal(self, ordinal: u32) -> bool {
        match self {
            Self::Arabic => true,
            Self::LowerAlpha | Self::UpperAlpha => (1..=MAX_ALPHA).contains(&ordinal),
            Self::LowerRoman | Self::UpperRoman => (1..=MAX_ROMAN).contains(&ordinal),
        }
    }

    /// The HTML class Sphinx puts on the `<ol>` for this sequence.
    #[must_use]
    pub const fn css_class(self) -> &'static str {
        match self {
            Self::Arabic => "arabic",
            Self::LowerAlpha => "loweralpha",
            Self::UpperAlpha => "upperalpha",
            Self::LowerRoman => "lowerroman",
            Self::UpperRoman => "upperroman",
        }
    }

    /// The CSS `list-style-type` keyword corresponding to this sequence, used
    /// by the renderer's `counter()` calls for the parenthesised formats.
    #[must_use]
    pub const fn list_style_type(self) -> &'static str {
        match self {
            Self::Arabic => "decimal",
            Self::LowerAlpha => "lower-alpha",
            Self::UpperAlpha => "upper-alpha",
            Self::LowerRoman => "lower-roman",
            Self::UpperRoman => "upper-roman",
        }
    }
}

/// Converts a single letter to its 1-based position in the alphabet.
fn alpha_ordinal(text: &str, base: char) -> Option<u32> {
    let c = text.chars().next()?;
    u32::from(c).checked_sub(u32::from(base))?.checked_add(1)
}

/// Converts a 1-based alphabet position to its letter, or `None` past `z`/`Z`.
fn alpha_text(ordinal: u32, base: char) -> Option<String> {
    if !(1..=MAX_ALPHA).contains(&ordinal) {
        return None;
    }
    char::from_u32(u32::from(base) + ordinal - 1).map(String::from)
}

/// The value/symbol table for roman numerals, largest first, including the
/// subtractive pairs. Ordering is what makes [`to_roman`] emit canonical form.
const ROMAN_VALUES: [(u32, &str); 13] = [
    (1000, "M"),
    (900, "CM"),
    (500, "D"),
    (400, "CD"),
    (100, "C"),
    (90, "XC"),
    (50, "L"),
    (40, "XL"),
    (10, "X"),
    (9, "IX"),
    (5, "V"),
    (4, "IV"),
    (1, "I"),
];

/// Renders `ordinal` as an uppercase roman numeral, or `None` outside 1..=4999.
fn to_roman(ordinal: u32) -> Option<String> {
    if !(1..=MAX_ROMAN).contains(&ordinal) {
        return None;
    }
    let mut remaining = ordinal;
    let mut out = String::new();
    for (value, symbol) in ROMAN_VALUES {
        while remaining >= value {
            out.push_str(symbol);
            remaining -= value;
        }
    }
    Some(out)
}

/// Parses an uppercase roman numeral, accepting canonical spellings only.
///
/// Canonicality is checked by re-rendering the parsed ordinal and comparing —
/// cheaper to trust than a hand-written grammar, and it rejects exactly what
/// docutils' `roman.fromRoman` rejects (`IIII`, `VX`, `IC`, …).
fn from_roman(text: &str) -> Option<u32> {
    let mut total: u32 = 0;
    let mut rest = text;
    for (value, symbol) in ROMAN_VALUES {
        while let Some(tail) = rest.strip_prefix(symbol) {
            total = total.checked_add(value)?;
            rest = tail;
        }
    }
    if !rest.is_empty() || total == 0 {
        return None;
    }
    (to_roman(total)? == text).then_some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_accepts_each_sequences_own_shape() {
        // Given the canonical shape of each sequence
        let cases = [
            (EnumeratorSequence::Arabic, "42"),
            (EnumeratorSequence::LowerAlpha, "q"),
            (EnumeratorSequence::UpperAlpha, "Q"),
            (EnumeratorSequence::LowerRoman, "xiv"),
            (EnumeratorSequence::UpperRoman, "XIV"),
        ];

        // When testing each against its own sequence
        // Then every one matches
        for (sequence, text) in cases {
            assert!(sequence.matches(text), "{sequence:?} should match {text}");
        }
    }

    #[test]
    fn test_matches_rejects_multi_character_alphabetic_text() {
        // Given a two-letter text, which docutils' single-character alpha
        // pattern does not admit
        let text = "ab";

        // When testing it against the alphabetic sequences
        // Then neither matches
        assert!(!EnumeratorSequence::LowerAlpha.matches(text));
        assert!(!EnumeratorSequence::UpperAlpha.matches("AB"));
    }

    #[test]
    fn test_matches_rejects_empty_text() {
        // Given empty enumerator text
        // When testing it against every sequence
        // Then none matches
        for sequence in EnumeratorSequence::RESOLUTION_ORDER {
            assert!(!sequence.matches(""));
        }
    }

    #[test]
    fn test_matches_rejects_letters_outside_the_roman_alphabet() {
        // Given text using a letter that is not a roman symbol
        let text = "ab";

        // When testing it against the roman sequences
        // Then it does not match
        assert!(!EnumeratorSequence::LowerRoman.matches(text));
    }

    #[test]
    fn test_ordinal_of_converts_each_sequence() {
        // Given one enumerator text per sequence
        let cases = [
            (EnumeratorSequence::Arabic, "42", 42),
            (EnumeratorSequence::LowerAlpha, "a", 1),
            (EnumeratorSequence::LowerAlpha, "z", 26),
            (EnumeratorSequence::UpperAlpha, "C", 3),
            (EnumeratorSequence::LowerRoman, "xiv", 14),
            (EnumeratorSequence::UpperRoman, "MCMXCIX", 1999),
        ];

        // When converting each to an ordinal
        // Then the expected number comes back
        for (sequence, text, expected) in cases {
            assert_eq!(sequence.ordinal_of(text), Some(expected), "{text}");
        }
    }

    #[test]
    fn test_ordinal_of_accepts_arabic_zero() {
        // Given the literal `0`, which docutils accepts as an arabic enumerator
        // When converting it
        // Then it yields ordinal zero rather than failing
        assert_eq!(EnumeratorSequence::Arabic.ordinal_of("0"), Some(0));
    }

    #[test]
    fn test_ordinal_of_rejects_non_canonical_roman_numerals() {
        // Given roman spellings that are readable but not canonical
        let non_canonical = ["IIII", "VIIII", "XXXX", "IC", "VX"];

        // When converting each
        // Then none yields an ordinal
        for text in non_canonical {
            assert_eq!(
                EnumeratorSequence::UpperRoman.ordinal_of(text),
                None,
                "{text}"
            );
        }
    }

    #[test]
    fn test_ordinal_of_rejects_roman_numerals_above_the_range() {
        // Given a roman spelling exceeding 4999
        let text = "MMMMM";

        // When converting it
        // Then it has no ordinal
        assert_eq!(EnumeratorSequence::UpperRoman.ordinal_of(text), None);
    }

    #[test]
    fn test_ordinal_of_rejects_text_from_a_different_sequence() {
        // Given text that belongs to another sequence
        // When converting it against the wrong sequence
        // Then it has no ordinal
        assert_eq!(EnumeratorSequence::Arabic.ordinal_of("iv"), None);
        assert_eq!(EnumeratorSequence::LowerRoman.ordinal_of("42"), None);
    }

    #[test]
    fn test_render_and_ordinal_of_round_trip_over_the_whole_valid_range() {
        // Given every sequence and every ordinal it accepts
        for sequence in EnumeratorSequence::RESOLUTION_ORDER {
            let last = match sequence {
                EnumeratorSequence::Arabic => 5000,
                EnumeratorSequence::LowerAlpha | EnumeratorSequence::UpperAlpha => MAX_ALPHA,
                EnumeratorSequence::LowerRoman | EnumeratorSequence::UpperRoman => MAX_ROMAN,
            };

            // When rendering an ordinal and converting it back
            for ordinal in 1..=last {
                let text = sequence
                    .render(ordinal)
                    .unwrap_or_else(|| panic!("{sequence:?} should render {ordinal}"));

                // Then the original ordinal comes back
                assert_eq!(
                    sequence.ordinal_of(&text),
                    Some(ordinal),
                    "{sequence:?} {text}"
                );
            }
        }
    }

    #[test]
    fn test_render_returns_none_past_each_sequences_ceiling() {
        // Given the first ordinal past each bounded sequence's ceiling
        // When rendering it
        // Then nothing comes back
        assert_eq!(EnumeratorSequence::LowerAlpha.render(MAX_ALPHA + 1), None);
        assert_eq!(EnumeratorSequence::UpperAlpha.render(MAX_ALPHA + 1), None);
        assert_eq!(EnumeratorSequence::LowerRoman.render(MAX_ROMAN + 1), None);
        assert_eq!(EnumeratorSequence::UpperRoman.render(MAX_ROMAN + 1), None);
    }

    #[test]
    fn test_render_returns_none_for_ordinal_zero_outside_arabic() {
        // Given ordinal zero, which only arabic can express
        // When rendering it
        // Then only arabic produces text
        assert_eq!(EnumeratorSequence::Arabic.render(0), Some("0".to_string()));
        assert_eq!(EnumeratorSequence::LowerAlpha.render(0), None);
        assert_eq!(EnumeratorSequence::LowerRoman.render(0), None);
    }

    #[test]
    fn test_accepts_ordinal_agrees_with_render_across_the_boundaries() {
        // Given the ordinals either side of every sequence's ceiling
        for sequence in EnumeratorSequence::RESOLUTION_ORDER {
            for ordinal in [0, 1, MAX_ALPHA, MAX_ALPHA + 1, MAX_ROMAN, MAX_ROMAN + 1] {
                // When asking whether the sequence accepts it
                // Then the answer matches whether it can be rendered
                assert_eq!(
                    sequence.accepts_ordinal(ordinal),
                    sequence.render(ordinal).is_some(),
                    "{sequence:?} {ordinal}"
                );
            }
        }
    }

    #[test]
    fn test_resolution_order_puts_alpha_before_roman() {
        // Given the resolution order docutils uses
        let order = EnumeratorSequence::RESOLUTION_ORDER;

        // When looking for the first sequence matching the ambiguous text `v`
        let first = order.into_iter().find(|s| s.matches("v"));

        // Then it resolves to lower-alpha, not roman five
        assert_eq!(first, Some(EnumeratorSequence::LowerAlpha));
    }

    #[test]
    fn test_css_class_and_list_style_type_are_distinct_per_sequence() {
        // Given every sequence
        let order = EnumeratorSequence::RESOLUTION_ORDER;

        // When collecting their CSS names
        let classes: Vec<_> = order.iter().map(|s| s.css_class()).collect();
        let styles: Vec<_> = order.iter().map(|s| s.list_style_type()).collect();

        // Then no two sequences share a name in either vocabulary
        for names in [&classes, &styles] {
            let mut sorted = names.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), names.len(), "{names:?}");
        }
    }

    #[test]
    fn test_alpha_ordinal_maps_a_letter_to_its_alphabet_position() {
        // Given the first and last letters of each case
        // When converting them
        // Then they map to 1 and 26
        assert_eq!(alpha_ordinal("a", 'a'), Some(1));
        assert_eq!(alpha_ordinal("z", 'a'), Some(26));
        assert_eq!(alpha_ordinal("A", 'A'), Some(1));
    }

    #[test]
    fn test_alpha_ordinal_returns_none_for_empty_text() {
        // Given empty text
        // When converting it
        // Then nothing comes back rather than panicking
        assert_eq!(alpha_ordinal("", 'a'), None);
    }

    #[test]
    fn test_alpha_text_renders_only_within_the_alphabet() {
        // Given ordinals inside and outside the alphabet
        // When rendering them
        // Then only the in-range ones produce a letter
        assert_eq!(alpha_text(1, 'a').as_deref(), Some("a"));
        assert_eq!(alpha_text(MAX_ALPHA, 'A').as_deref(), Some("Z"));
        assert_eq!(alpha_text(0, 'a'), None);
        assert_eq!(alpha_text(MAX_ALPHA + 1, 'a'), None);
    }

    #[test]
    fn test_to_roman_uses_the_subtractive_forms() {
        // Given ordinals whose canonical spelling is subtractive
        // When rendering them
        // Then the compact form is produced, not the additive one
        assert_eq!(to_roman(4).as_deref(), Some("IV"));
        assert_eq!(to_roman(9).as_deref(), Some("IX"));
        assert_eq!(to_roman(1990).as_deref(), Some("MCMXC"));
        assert_eq!(to_roman(MAX_ROMAN).as_deref(), Some("MMMMCMXCIX"));
    }

    #[test]
    fn test_to_roman_returns_none_outside_its_range() {
        // Given ordinals either side of the representable range
        // When rendering them
        // Then nothing comes back
        assert_eq!(to_roman(0), None);
        assert_eq!(to_roman(MAX_ROMAN + 1), None);
    }

    #[test]
    fn test_from_roman_rejects_trailing_junk() {
        // Given a valid numeral followed by a character it cannot consume
        let text = "XIQ";

        // When parsing it
        // Then it is rejected rather than parsed as its valid prefix
        assert_eq!(from_roman(text), None);
    }

    #[test]
    fn test_from_roman_rejects_empty_text() {
        // Given empty text
        // When parsing it
        // Then nothing comes back
        assert_eq!(from_roman(""), None);
    }

    #[test]
    fn test_enum_sequence_serialization_roundtrip() {
        // Given every sequence
        for sequence in EnumeratorSequence::RESOLUTION_ORDER {
            // When serializing and deserializing it
            let json = serde_json::to_string(&sequence).expect("Failed to serialize");
            let deserialized: EnumeratorSequence =
                serde_json::from_str(&json).expect("Failed to deserialize");

            // Then the value survives the round trip
            assert_eq!(sequence, deserialized);
        }
    }
}
