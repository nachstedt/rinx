//! The character classes RST's inline-markup recognition rules are phrased in,
//! ported from docutils' `docutils/utils/punctuation_chars.py`.
//!
//! docutils decides whether a `*`, `**` or ``` `` ``` is really markup from the
//! characters on either side of it: a start-string has to open at the start of
//! the text or directly after whitespace, an *opener* or a *delimiter*, and an
//! end-string has to be followed by the end of the text, whitespace, a *closer*,
//! a *delimiter* or a *closing delimiter*. That is what makes `mid*word*markup`
//! plain text while `(*this*)` is emphasis.
//!
//! The tables below are transcribed rather than derived from Unicode general
//! categories at runtime, because docutils itself pre-generates and freezes them
//! "to prevent dependence on the Python version". Copying the frozen tables is
//! therefore the faithful port, not a shortcut — deriving them ourselves would
//! drift against docutils whenever either side's Unicode version moved.
//!
//! Note that docutils stores these as regular-expression character classes, so
//! its published lengths count range syntax rather than characters; expanding
//! `delimiters` yields 397 characters, not the 247 its source string suggests.

use super::escapes::MARKER;

/// Membership test over a table sorted by range start.
fn in_ranges(c: char, ranges: &[(char, char)]) -> bool {
    ranges
        .binary_search_by(|&(low, high)| {
            if c < low {
                std::cmp::Ordering::Greater
            } else if c > high {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// Whether `c` may sit directly before a markup start-string.
///
/// docutils' `start_string_prefix`. A backslash is deliberately in neither
/// class, which is why `a \\*x*` renders a literal `\*x*` rather than emphasis:
/// the surviving backslash cannot open markup.
pub(super) fn can_precede_start_string(c: char) -> bool {
    c.is_whitespace() || in_ranges(c, OPENERS) || in_ranges(c, DELIMITERS)
}

/// Whether `c` may sit directly after a markup end-string.
///
/// docutils' `end_string_suffix`, which includes the escape marker: `*emph*\ x`
/// has to close so that the escaped space can then join the two together.
pub(super) fn can_follow_end_string(c: char) -> bool {
    c == MARKER
        || c.is_whitespace()
        || in_ranges(c, CLOSERS)
        || in_ranges(c, DELIMITERS)
        || in_ranges(c, CLOSING_DELIMITERS)
}

/// Opening delimiters: a markup start-string may directly follow one.
/// Expanded from docutils' `punctuation_chars.openers` (97 characters in 80 ranges).
static OPENERS: &[(char, char)] = &[
    ('"', '"'),
    ('\'', '('),
    ('<', '<'),
    ('[', '['),
    ('{', '{'),
    ('\u{ab}', '\u{ab}'),
    ('\u{bb}', '\u{bb}'),
    ('\u{f3a}', '\u{f3a}'),
    ('\u{f3c}', '\u{f3c}'),
    ('\u{169b}', '\u{169b}'),
    ('\u{2018}', '\u{201f}'),
    ('\u{2039}', '\u{203a}'),
    ('\u{2045}', '\u{2045}'),
    ('\u{207d}', '\u{207d}'),
    ('\u{208d}', '\u{208d}'),
    ('\u{2329}', '\u{2329}'),
    ('\u{2768}', '\u{2768}'),
    ('\u{276a}', '\u{276a}'),
    ('\u{276c}', '\u{276c}'),
    ('\u{276e}', '\u{276e}'),
    ('\u{2770}', '\u{2770}'),
    ('\u{2772}', '\u{2772}'),
    ('\u{2774}', '\u{2774}'),
    ('\u{27c5}', '\u{27c5}'),
    ('\u{27e6}', '\u{27e6}'),
    ('\u{27e8}', '\u{27e8}'),
    ('\u{27ea}', '\u{27ea}'),
    ('\u{27ec}', '\u{27ec}'),
    ('\u{27ee}', '\u{27ee}'),
    ('\u{2983}', '\u{2983}'),
    ('\u{2985}', '\u{2985}'),
    ('\u{2987}', '\u{2987}'),
    ('\u{2989}', '\u{2989}'),
    ('\u{298b}', '\u{298b}'),
    ('\u{298d}', '\u{298d}'),
    ('\u{298f}', '\u{298f}'),
    ('\u{2991}', '\u{2991}'),
    ('\u{2993}', '\u{2993}'),
    ('\u{2995}', '\u{2995}'),
    ('\u{2997}', '\u{2997}'),
    ('\u{29d8}', '\u{29d8}'),
    ('\u{29da}', '\u{29da}'),
    ('\u{29fc}', '\u{29fc}'),
    ('\u{2e02}', '\u{2e05}'),
    ('\u{2e09}', '\u{2e0a}'),
    ('\u{2e0c}', '\u{2e0d}'),
    ('\u{2e1c}', '\u{2e1d}'),
    ('\u{2e20}', '\u{2e22}'),
    ('\u{2e24}', '\u{2e24}'),
    ('\u{2e26}', '\u{2e26}'),
    ('\u{2e28}', '\u{2e28}'),
    ('\u{3008}', '\u{3008}'),
    ('\u{300a}', '\u{300a}'),
    ('\u{300c}', '\u{300c}'),
    ('\u{300e}', '\u{300e}'),
    ('\u{3010}', '\u{3010}'),
    ('\u{3014}', '\u{3014}'),
    ('\u{3016}', '\u{3016}'),
    ('\u{3018}', '\u{3018}'),
    ('\u{301a}', '\u{301a}'),
    ('\u{301d}', '\u{301d}'),
    ('\u{fd3e}', '\u{fd3e}'),
    ('\u{fe17}', '\u{fe17}'),
    ('\u{fe35}', '\u{fe35}'),
    ('\u{fe37}', '\u{fe37}'),
    ('\u{fe39}', '\u{fe39}'),
    ('\u{fe3b}', '\u{fe3b}'),
    ('\u{fe3d}', '\u{fe3d}'),
    ('\u{fe3f}', '\u{fe3f}'),
    ('\u{fe41}', '\u{fe41}'),
    ('\u{fe43}', '\u{fe43}'),
    ('\u{fe47}', '\u{fe47}'),
    ('\u{fe59}', '\u{fe59}'),
    ('\u{fe5b}', '\u{fe5b}'),
    ('\u{fe5d}', '\u{fe5d}'),
    ('\u{ff08}', '\u{ff08}'),
    ('\u{ff3b}', '\u{ff3b}'),
    ('\u{ff5b}', '\u{ff5b}'),
    ('\u{ff5f}', '\u{ff5f}'),
    ('\u{ff62}', '\u{ff62}'),
];

/// Closing delimiters: a markup end-string may be directly followed by one.
/// Expanded from docutils' `punctuation_chars.closers` (98 characters in 82 ranges).
static CLOSERS: &[(char, char)] = &[
    ('"', '"'),
    ('\'', '\''),
    (')', ')'),
    ('>', '>'),
    (']', ']'),
    ('}', '}'),
    ('\u{ab}', '\u{ab}'),
    ('\u{bb}', '\u{bb}'),
    ('\u{f3b}', '\u{f3b}'),
    ('\u{f3d}', '\u{f3d}'),
    ('\u{169c}', '\u{169c}'),
    ('\u{2018}', '\u{201f}'),
    ('\u{2039}', '\u{203a}'),
    ('\u{2046}', '\u{2046}'),
    ('\u{207e}', '\u{207e}'),
    ('\u{208e}', '\u{208e}'),
    ('\u{232a}', '\u{232a}'),
    ('\u{2769}', '\u{2769}'),
    ('\u{276b}', '\u{276b}'),
    ('\u{276d}', '\u{276d}'),
    ('\u{276f}', '\u{276f}'),
    ('\u{2771}', '\u{2771}'),
    ('\u{2773}', '\u{2773}'),
    ('\u{2775}', '\u{2775}'),
    ('\u{27c6}', '\u{27c6}'),
    ('\u{27e7}', '\u{27e7}'),
    ('\u{27e9}', '\u{27e9}'),
    ('\u{27eb}', '\u{27eb}'),
    ('\u{27ed}', '\u{27ed}'),
    ('\u{27ef}', '\u{27ef}'),
    ('\u{2984}', '\u{2984}'),
    ('\u{2986}', '\u{2986}'),
    ('\u{2988}', '\u{2988}'),
    ('\u{298a}', '\u{298a}'),
    ('\u{298c}', '\u{298c}'),
    ('\u{298e}', '\u{298e}'),
    ('\u{2990}', '\u{2990}'),
    ('\u{2992}', '\u{2992}'),
    ('\u{2994}', '\u{2994}'),
    ('\u{2996}', '\u{2996}'),
    ('\u{2998}', '\u{2998}'),
    ('\u{29d9}', '\u{29d9}'),
    ('\u{29db}', '\u{29db}'),
    ('\u{29fd}', '\u{29fd}'),
    ('\u{2e02}', '\u{2e05}'),
    ('\u{2e09}', '\u{2e0a}'),
    ('\u{2e0c}', '\u{2e0d}'),
    ('\u{2e1c}', '\u{2e1d}'),
    ('\u{2e20}', '\u{2e21}'),
    ('\u{2e23}', '\u{2e23}'),
    ('\u{2e25}', '\u{2e25}'),
    ('\u{2e27}', '\u{2e27}'),
    ('\u{2e29}', '\u{2e29}'),
    ('\u{3009}', '\u{3009}'),
    ('\u{300b}', '\u{300b}'),
    ('\u{300d}', '\u{300d}'),
    ('\u{300f}', '\u{300f}'),
    ('\u{3011}', '\u{3011}'),
    ('\u{3015}', '\u{3015}'),
    ('\u{3017}', '\u{3017}'),
    ('\u{3019}', '\u{3019}'),
    ('\u{301b}', '\u{301b}'),
    ('\u{301e}', '\u{301f}'),
    ('\u{fd3f}', '\u{fd3f}'),
    ('\u{fe18}', '\u{fe18}'),
    ('\u{fe36}', '\u{fe36}'),
    ('\u{fe38}', '\u{fe38}'),
    ('\u{fe3a}', '\u{fe3a}'),
    ('\u{fe3c}', '\u{fe3c}'),
    ('\u{fe3e}', '\u{fe3e}'),
    ('\u{fe40}', '\u{fe40}'),
    ('\u{fe42}', '\u{fe42}'),
    ('\u{fe44}', '\u{fe44}'),
    ('\u{fe48}', '\u{fe48}'),
    ('\u{fe5a}', '\u{fe5a}'),
    ('\u{fe5c}', '\u{fe5c}'),
    ('\u{fe5e}', '\u{fe5e}'),
    ('\u{ff09}', '\u{ff09}'),
    ('\u{ff3d}', '\u{ff3d}'),
    ('\u{ff5d}', '\u{ff5d}'),
    ('\u{ff60}', '\u{ff60}'),
    ('\u{ff63}', '\u{ff63}'),
];

/// Delimiters: may precede a start-string or follow an end-string.
/// Expanded from docutils' `punctuation_chars.delimiters` (397 characters in 121 ranges).
static DELIMITERS: &[(char, char)] = &[
    ('-', '-'),
    ('/', '/'),
    (':', ':'),
    ('\u{a1}', '\u{a1}'),
    ('\u{b7}', '\u{b7}'),
    ('\u{bf}', '\u{bf}'),
    ('\u{37e}', '\u{37e}'),
    ('\u{387}', '\u{387}'),
    ('\u{55a}', '\u{55f}'),
    ('\u{589}', '\u{58a}'),
    ('\u{5be}', '\u{5be}'),
    ('\u{5c0}', '\u{5c0}'),
    ('\u{5c3}', '\u{5c3}'),
    ('\u{5c6}', '\u{5c6}'),
    ('\u{5f3}', '\u{5f4}'),
    ('\u{609}', '\u{60a}'),
    ('\u{60c}', '\u{60d}'),
    ('\u{61b}', '\u{61b}'),
    ('\u{61e}', '\u{61f}'),
    ('\u{66a}', '\u{66d}'),
    ('\u{6d4}', '\u{6d4}'),
    ('\u{700}', '\u{70d}'),
    ('\u{7f7}', '\u{7f9}'),
    ('\u{830}', '\u{83e}'),
    ('\u{964}', '\u{965}'),
    ('\u{970}', '\u{970}'),
    ('\u{df4}', '\u{df4}'),
    ('\u{e4f}', '\u{e4f}'),
    ('\u{e5a}', '\u{e5b}'),
    ('\u{f04}', '\u{f12}'),
    ('\u{f85}', '\u{f85}'),
    ('\u{fd0}', '\u{fd4}'),
    ('\u{104a}', '\u{104f}'),
    ('\u{10fb}', '\u{10fb}'),
    ('\u{1361}', '\u{1368}'),
    ('\u{1400}', '\u{1400}'),
    ('\u{166d}', '\u{166e}'),
    ('\u{16eb}', '\u{16ed}'),
    ('\u{1735}', '\u{1736}'),
    ('\u{17d4}', '\u{17d6}'),
    ('\u{17d8}', '\u{17da}'),
    ('\u{1800}', '\u{180a}'),
    ('\u{1944}', '\u{1945}'),
    ('\u{19de}', '\u{19df}'),
    ('\u{1a1e}', '\u{1a1f}'),
    ('\u{1aa0}', '\u{1aa6}'),
    ('\u{1aa8}', '\u{1aad}'),
    ('\u{1b5a}', '\u{1b60}'),
    ('\u{1c3b}', '\u{1c3f}'),
    ('\u{1c7e}', '\u{1c7f}'),
    ('\u{1cd3}', '\u{1cd3}'),
    ('\u{2010}', '\u{2017}'),
    ('\u{2020}', '\u{2027}'),
    ('\u{2030}', '\u{2038}'),
    ('\u{203b}', '\u{203e}'),
    ('\u{2041}', '\u{2043}'),
    ('\u{2047}', '\u{2051}'),
    ('\u{2053}', '\u{2053}'),
    ('\u{2055}', '\u{205e}'),
    ('\u{2cf9}', '\u{2cfc}'),
    ('\u{2cfe}', '\u{2cff}'),
    ('\u{2e00}', '\u{2e01}'),
    ('\u{2e06}', '\u{2e08}'),
    ('\u{2e0b}', '\u{2e0b}'),
    ('\u{2e0e}', '\u{2e1b}'),
    ('\u{2e1e}', '\u{2e1f}'),
    ('\u{2e2a}', '\u{2e2e}'),
    ('\u{2e30}', '\u{2e31}'),
    ('\u{3001}', '\u{3003}'),
    ('\u{301c}', '\u{301c}'),
    ('\u{3030}', '\u{3030}'),
    ('\u{303d}', '\u{303d}'),
    ('\u{30a0}', '\u{30a0}'),
    ('\u{30fb}', '\u{30fb}'),
    ('\u{a4fe}', '\u{a4ff}'),
    ('\u{a60d}', '\u{a60f}'),
    ('\u{a673}', '\u{a673}'),
    ('\u{a67e}', '\u{a67e}'),
    ('\u{a6f2}', '\u{a6f7}'),
    ('\u{a874}', '\u{a877}'),
    ('\u{a8ce}', '\u{a8cf}'),
    ('\u{a8f8}', '\u{a8fa}'),
    ('\u{a92e}', '\u{a92f}'),
    ('\u{a95f}', '\u{a95f}'),
    ('\u{a9c1}', '\u{a9cd}'),
    ('\u{a9de}', '\u{a9df}'),
    ('\u{aa5c}', '\u{aa5f}'),
    ('\u{aade}', '\u{aadf}'),
    ('\u{abeb}', '\u{abeb}'),
    ('\u{fe10}', '\u{fe16}'),
    ('\u{fe19}', '\u{fe19}'),
    ('\u{fe30}', '\u{fe32}'),
    ('\u{fe45}', '\u{fe46}'),
    ('\u{fe49}', '\u{fe4c}'),
    ('\u{fe50}', '\u{fe52}'),
    ('\u{fe54}', '\u{fe58}'),
    ('\u{fe5f}', '\u{fe61}'),
    ('\u{fe63}', '\u{fe63}'),
    ('\u{fe68}', '\u{fe68}'),
    ('\u{fe6a}', '\u{fe6b}'),
    ('\u{ff01}', '\u{ff03}'),
    ('\u{ff05}', '\u{ff07}'),
    ('\u{ff0a}', '\u{ff0a}'),
    ('\u{ff0c}', '\u{ff0f}'),
    ('\u{ff1a}', '\u{ff1b}'),
    ('\u{ff1f}', '\u{ff20}'),
    ('\u{ff3c}', '\u{ff3c}'),
    ('\u{ff61}', '\u{ff61}'),
    ('\u{ff64}', '\u{ff65}'),
    ('\u{10100}', '\u{10101}'),
    ('\u{1039f}', '\u{1039f}'),
    ('\u{103d0}', '\u{103d0}'),
    ('\u{10857}', '\u{10857}'),
    ('\u{1091f}', '\u{1091f}'),
    ('\u{1093f}', '\u{1093f}'),
    ('\u{10a50}', '\u{10a58}'),
    ('\u{10a7f}', '\u{10a7f}'),
    ('\u{10b39}', '\u{10b3f}'),
    ('\u{110bb}', '\u{110bc}'),
    ('\u{110be}', '\u{110c1}'),
    ('\u{12470}', '\u{12473}'),
];

/// Closing delimiters proper: may follow an end-string.
/// Expanded from docutils' `punctuation_chars.closing_delimiters` (6 characters in 6 ranges).
static CLOSING_DELIMITERS: &[(char, char)] = &[
    ('!', '!'),
    (',', ','),
    ('.', '.'),
    (';', ';'),
    ('?', '?'),
    ('\\', '\\'),
];

#[cfg(test)]
mod tests {
    use super::*;

    // --- in_ranges ---

    #[test]
    fn test_in_ranges_finds_a_character_inside_a_range() {
        // Given / When / Then
        assert!(in_ranges('c', &[('a', 'e')]));
    }

    #[test]
    fn test_in_ranges_finds_a_range_boundary() {
        // Given / When / Then
        assert!(in_ranges('a', &[('a', 'e')]) && in_ranges('e', &[('a', 'e')]));
    }

    #[test]
    fn test_in_ranges_rejects_a_character_outside_every_range() {
        // Given / When / Then
        assert!(!in_ranges('z', &[('a', 'e'), ('g', 'k')]));
    }

    #[test]
    fn test_in_ranges_searches_beyond_the_first_range() {
        // Given / When / Then — guards the binary search, not just a scan
        assert!(in_ranges('h', &[('a', 'e'), ('g', 'k')]));
    }

    // --- can_precede_start_string ---

    #[test]
    fn test_can_precede_start_string_accepts_whitespace() {
        // Given / When / Then
        assert!(can_precede_start_string(' '));
    }

    #[test]
    fn test_can_precede_start_string_accepts_ascii_openers() {
        // Given / When / Then — the set the previous hand-rolled check listed
        for c in ['(', '[', '{', '<', '"', '\''] {
            assert!(can_precede_start_string(c), "rejected opener {c:?}");
        }
    }

    #[test]
    fn test_can_precede_start_string_accepts_ascii_delimiters() {
        // Given / When / Then
        for c in ['-', '/', ':'] {
            assert!(can_precede_start_string(c), "rejected delimiter {c:?}");
        }
    }

    #[test]
    fn test_can_precede_start_string_accepts_unicode_openers() {
        // Given / When / Then — coverage the ASCII-only check could not express
        for c in ['\u{2018}', '\u{201c}', '\u{ab}'] {
            assert!(can_precede_start_string(c), "rejected opener {c:?}");
        }
    }

    #[test]
    fn test_can_precede_start_string_rejects_a_letter() {
        // Given / When / Then — this is what makes `mid*word*` plain text
        assert!(!can_precede_start_string('d'));
    }

    #[test]
    fn test_can_precede_start_string_rejects_a_backslash() {
        // Given / When / Then — why `a \\*x*` yields a literal `\*x*`
        assert!(!can_precede_start_string('\\'));
    }

    // --- can_follow_end_string ---

    #[test]
    fn test_can_follow_end_string_accepts_the_escape_marker() {
        // Given / When / Then — lets `*emph*\ x` close before the escaped
        // space joins it to what follows
        assert!(can_follow_end_string(MARKER));
    }

    #[test]
    fn test_can_follow_end_string_accepts_whitespace_and_ascii_closers() {
        // Given / When / Then
        for c in [' ', ')', ']', '}', '"', '\''] {
            assert!(can_follow_end_string(c), "rejected closer {c:?}");
        }
    }

    #[test]
    fn test_can_follow_end_string_accepts_ascii_closing_delimiters() {
        // Given / When / Then
        for c in ['.', ',', ';', '!', '?', '\\'] {
            assert!(can_follow_end_string(c), "rejected closing delimiter {c:?}");
        }
    }

    #[test]
    fn test_can_follow_end_string_accepts_unicode_punctuation() {
        // Given / When / Then — an em dash or curly quote after emphasis, which
        // the previous ASCII-only set rejected
        for c in ['\u{2014}', '\u{2013}', '\u{2019}', '\u{201d}'] {
            assert!(can_follow_end_string(c), "rejected {c:?}");
        }
    }

    #[test]
    fn test_can_follow_end_string_rejects_a_letter() {
        // Given / When / Then
        assert!(!can_follow_end_string('x'));
    }

    // --- the tables themselves ---

    #[test]
    fn test_tables_are_sorted_and_non_overlapping() {
        // Given each table, whose ordering the binary search depends on
        for (name, table) in [
            ("OPENERS", OPENERS),
            ("CLOSERS", CLOSERS),
            ("DELIMITERS", DELIMITERS),
            ("CLOSING_DELIMITERS", CLOSING_DELIMITERS),
        ] {
            // Then
            for pair in table.windows(2) {
                assert!(
                    pair[0].1 < pair[1].0,
                    "{name} is unsorted or overlapping at {:?}/{:?}",
                    pair[0],
                    pair[1]
                );
            }
            for &(low, high) in table {
                assert!(
                    low <= high,
                    "{name} has an inverted range {low:?}..{high:?}"
                );
            }
        }
    }

    #[test]
    fn test_tables_hold_the_character_counts_docutils_expands_to() {
        // Given the expanded sizes of docutils' own classes, so a botched
        // transcription cannot pass unnoticed
        for (name, table, expected) in [
            ("OPENERS", OPENERS, 97),
            ("CLOSERS", CLOSERS, 98),
            ("DELIMITERS", DELIMITERS, 397),
            ("CLOSING_DELIMITERS", CLOSING_DELIMITERS, 6),
        ] {
            // When
            let count: usize = table
                .iter()
                .map(|&(low, high)| (u32::from(high) - u32::from(low) + 1) as usize)
                .sum();

            // Then
            assert_eq!(count, expected, "{name} has the wrong character count");
        }
    }
}
