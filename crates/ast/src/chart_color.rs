//! A colour a chart may be drawn with.
//!
//! "Parse, don't validate": the type can only hold a colour this build can
//! actually draw, so the renderer never has to ask whether a `:colors:` entry
//! made sense. The check happens where the option line is — the only phase
//! that can point at the characters the author wrote — and the stored form is
//! already the three channels a drawing backend wants.
//!
//! The accepted spellings are CSS hex (`#rgb`, `#rrggbb`) plus the sixteen
//! basic CSS colour keywords. That is deliberately narrower than matplotlib,
//! which is what sphinx-needs' `needpie` passes its `:colors:` to: the long
//! tail of matplotlib's names (`xkcd:baby poop green`, the `C0`–`C9` cycle) has
//! no meaning outside it, and silently drawing a wedge the wrong colour is
//! worse than saying which name was not understood.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

/// The sixteen basic CSS colour keywords, with their channels.
///
/// The basic set rather than the full 148: these are the names an author
/// reaches for without looking anything up, and every other spelling has an
/// unambiguous hex form to write instead — which the diagnostic says.
const NAMED_COLORS: [(&str, (u8, u8, u8)); 16] = [
    ("black", (0x00, 0x00, 0x00)),
    ("silver", (0xc0, 0xc0, 0xc0)),
    ("gray", (0x80, 0x80, 0x80)),
    ("grey", (0x80, 0x80, 0x80)),
    ("white", (0xff, 0xff, 0xff)),
    ("maroon", (0x80, 0x00, 0x00)),
    ("red", (0xff, 0x00, 0x00)),
    ("purple", (0x80, 0x00, 0x80)),
    ("green", (0x00, 0x80, 0x00)),
    ("lime", (0x00, 0xff, 0x00)),
    ("olive", (0x80, 0x80, 0x00)),
    ("yellow", (0xff, 0xff, 0x00)),
    ("navy", (0x00, 0x00, 0x80)),
    ("blue", (0x00, 0x00, 0xff)),
    ("teal", (0x00, 0x80, 0x80)),
    ("aqua", (0x00, 0xff, 0xff)),
];

/// A colour a chart is drawn with, stored as its three channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ChartColor {
    red: u8,
    green: u8,
    blue: u8,
}

/// Why a written colour was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidChartColor(String);

impl fmt::Display for InvalidChartColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "'{}' is not a colour this build can draw with; write a hex colour such as #4c72b0, \
             or one of {}",
            self.0,
            NAMED_COLORS
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

impl ChartColor {
    /// Reads a written colour.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidChartColor`] for anything that is neither CSS hex nor
    /// one of the basic keywords, naming what was written.
    pub fn parse(value: &str) -> Result<Self, InvalidChartColor> {
        let written = value.trim();
        if let Some(hex) = written.strip_prefix('#') {
            return Self::from_hex(hex).ok_or_else(|| InvalidChartColor(written.to_string()));
        }
        let lowered = written.to_ascii_lowercase();
        NAMED_COLORS
            .iter()
            .find(|(name, _)| *name == lowered)
            .map(|(_, (red, green, blue))| Self {
                red: *red,
                green: *green,
                blue: *blue,
            })
            .ok_or_else(|| InvalidChartColor(written.to_string()))
    }

    /// The three channels, for whatever draws with them.
    #[must_use]
    pub const fn rgb(self) -> (u8, u8, u8) {
        (self.red, self.green, self.blue)
    }

    /// This colour as the `#rrggbb` an SVG attribute takes.
    #[must_use]
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.red, self.green, self.blue)
    }

    /// Reads the digits after a `#`, in either CSS length.
    fn from_hex(hex: &str) -> Option<Self> {
        let digits: Vec<u8> = hex
            .chars()
            .map(|c| c.to_digit(16).map(|d| u8::try_from(d).unwrap_or(0)))
            .collect::<Option<Vec<u8>>>()?;
        match digits.len() {
            // `#abc` is CSS shorthand for `#aabbcc`, so each digit doubles.
            3 => Some(Self {
                red: digits[0] * 0x11,
                green: digits[1] * 0x11,
                blue: digits[2] * 0x11,
            }),
            6 => Some(Self {
                red: digits[0] * 0x10 + digits[1],
                green: digits[2] * 0x10 + digits[3],
                blue: digits[4] * 0x10 + digits[5],
            }),
            _ => None,
        }
    }
}

impl<'de> Deserialize<'de> for ChartColor {
    /// Re-checks the invariant on load, as every opaque type here does: a
    /// `.ast` is a file on disk, and nothing guarantees this build wrote it.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            red: u8,
            green: u8,
            blue: u8,
        }
        let raw = Raw::deserialize(deserializer)?;
        Ok(Self {
            red: raw.red,
            green: raw.green,
            blue: raw.blue,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_six_digit_hex_colour_reads_its_three_channels() {
        // Given
        let value = "#4c72b0";

        // When
        let color = ChartColor::parse(value).expect("a valid colour");

        // Then
        assert_eq!(color.rgb(), (0x4c, 0x72, 0xb0));
    }

    #[test]
    fn test_a_three_digit_hex_colour_doubles_each_digit() {
        // Given — CSS shorthand, so `#abc` is `#aabbcc`
        let value = "#abc";

        // When
        let color = ChartColor::parse(value).expect("a valid colour");

        // Then
        assert_eq!(color.rgb(), (0xaa, 0xbb, 0xcc));
    }

    #[test]
    fn test_a_basic_keyword_reads_its_channels() {
        // Given
        let value = "Teal";

        // When
        let color = ChartColor::parse(value).expect("a valid colour");

        // Then — keywords are case-insensitive, as CSS's are
        assert_eq!(color.rgb(), (0x00, 0x80, 0x80));
    }

    #[test]
    fn test_a_matplotlib_only_name_is_refused_with_what_to_write() {
        // Given — sphinx-needs passes `:colors:` to matplotlib, whose long
        // tail of names means nothing here
        let value = "xkcd:baby poop green";

        // When
        let problem = ChartColor::parse(value).expect_err("expected a refusal");

        // Then
        assert!(
            problem.to_string().contains("xkcd:baby poop green"),
            "{problem}"
        );
        assert!(problem.to_string().contains("#4c72b0"), "{problem}");
    }

    #[test]
    fn test_a_hex_colour_of_the_wrong_length_is_refused() {
        // Given
        let value = "#12345";

        // When
        let problem = ChartColor::parse(value);

        // Then
        assert!(problem.is_err());
    }

    #[test]
    fn test_a_hex_colour_with_a_non_hex_digit_is_refused() {
        // Given
        let value = "#12zz34";

        // When
        let problem = ChartColor::parse(value);

        // Then
        assert!(problem.is_err());
    }

    #[test]
    fn test_a_colour_renders_as_the_hex_an_svg_attribute_takes() {
        // Given
        let color = ChartColor::parse("teal").unwrap();

        // When
        let hex = color.to_hex();

        // Then
        assert_eq!(hex, "#008080");
    }

    #[test]
    fn test_a_colour_survives_a_serialization_round_trip() {
        // Given — the node is written to a `.ast` and read back to render
        let color = ChartColor::parse("#4c72b0").unwrap();

        // When
        let json = serde_json::to_string(&color).unwrap();
        let decoded: ChartColor = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(decoded, color);
    }
}
