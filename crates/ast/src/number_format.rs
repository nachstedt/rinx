//! The text a number is shown in — `Fig. %s`, `Table {number}: {name}` — as
//! written in `rinx.toml`'s `[numfig_format]` or as a `:numref:` role's
//! explicit title.
//!
//! "Parse, don't validate": a [`NumberFormat`] is always one Sphinx could
//! apply, so whatever fills it in never meets a format it cannot use. Sphinx
//! applies one of two Python formatting styles, and which one is decided by a
//! substring test rather than by the syntax itself — this module reproduces
//! that test exactly, since it decides what a title like `only number`
//! shows:
//!
//! - **New style** (`str.format`) when the text contains `{name}` or the bare
//!   word `number` anywhere. `{number}` and `{name}` are the only fields;
//!   `{{`/`}}` are literal braces.
//! - **Old style** (`%`) otherwise. Exactly one `%s` is the number; `%%` is a
//!   literal percent sign.
//!
//! Everything Python would raise on — no `%s`, two of them, an unknown field,
//! an unbalanced brace — is refused here, where the author's text is read, and
//! named. Sphinx notices the same faults only while resolving the reference,
//! and some of them (an unbalanced brace) crash its build instead.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// One piece of a parsed format.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Text(String),
    Number,
    Name,
}

/// A format a number (and optionally a caption) is filled into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumberFormat {
    /// The text as written, kept for serialization and for showing a
    /// reference that could not be resolved.
    written: String,
    segments: Vec<Segment>,
}

/// Why a written format was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidNumberFormat {
    /// An old-style format with no `%s` for the number to go in.
    NoNumberSlot(String),
    /// An old-style format with more than one `%s`.
    SeveralNumberSlots(String),
    /// An old-style `%` conversion other than `%s` or `%%`.
    UnsupportedConversion(String),
    /// A new-style field other than `{number}` or `{name}`.
    UnknownField { format: String, field: String },
    /// A `{` or `}` that is neither a field nor doubled.
    UnbalancedBrace(String),
}

impl fmt::Display for InvalidNumberFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoNumberSlot(format) => write!(
                f,
                "'{format}' has no place for the number; write %s or {{number}} where it goes"
            ),
            Self::SeveralNumberSlots(format) => {
                write!(f, "'{format}' has more than one %s; write exactly one")
            }
            Self::UnsupportedConversion(format) => write!(
                f,
                "'{format}' uses a % conversion other than %s; write %% for a literal percent sign"
            ),
            Self::UnknownField { format, field } => write!(
                f,
                "'{format}' names the field {{{field}}}; only {{number}} and {{name}} exist"
            ),
            Self::UnbalancedBrace(format) => write!(
                f,
                "'{format}' has an unbalanced brace; write {{{{ or }}}} for a literal one"
            ),
        }
    }
}

impl std::error::Error for InvalidNumberFormat {}

impl NumberFormat {
    /// Reads a written format, deciding its style the way Sphinx does.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidNumberFormat`] for any format Python would refuse to
    /// apply.
    pub fn parse(written: &str) -> Result<Self, InvalidNumberFormat> {
        let segments = if written.contains("{name}") || written.contains("number") {
            parse_new_style(written)?
        } else {
            parse_old_style(written)?
        };
        Ok(Self {
            written: written.to_string(),
            segments,
        })
    }

    /// The format as it was written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.written
    }

    /// Whether this format shows the caption, so it cannot be applied to an
    /// element that has none.
    #[must_use]
    pub fn requires_name(&self) -> bool {
        self.segments.contains(&Segment::Name)
    }

    /// Fills `number` — already joined, e.g. `1.2` — and `name` in.
    ///
    /// A `{name}` field with no `name` supplied shows nothing; callers check
    /// [`Self::requires_name`] first, since that is a fault to report.
    #[must_use]
    pub fn apply(&self, number: &str, name: Option<&str>) -> String {
        self.segments
            .iter()
            .map(|segment| match segment {
                Segment::Text(text) => text.as_str(),
                Segment::Number => number,
                Segment::Name => name.unwrap_or_default(),
            })
            .collect()
    }
}

/// `str.format` restricted to the two fields Sphinx passes.
fn parse_new_style(written: &str) -> Result<Vec<Segment>, InvalidNumberFormat> {
    let mut segments = Vec::new();
    let mut text = String::new();
    let mut chars = written.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                text.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                text.push('}');
            }
            '{' => {
                let mut field = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some('{') | None => {
                            return Err(InvalidNumberFormat::UnbalancedBrace(written.to_string()));
                        }
                        Some(other) => field.push(other),
                    }
                }
                let segment = match field.as_str() {
                    "number" => Segment::Number,
                    "name" => Segment::Name,
                    _ => {
                        return Err(InvalidNumberFormat::UnknownField {
                            format: written.to_string(),
                            field,
                        });
                    }
                };
                push_text(&mut segments, &mut text);
                segments.push(segment);
            }
            '}' => return Err(InvalidNumberFormat::UnbalancedBrace(written.to_string())),
            other => text.push(other),
        }
    }
    push_text(&mut segments, &mut text);
    Ok(segments)
}

/// `%` formatting with the number as its one argument.
fn parse_old_style(written: &str) -> Result<Vec<Segment>, InvalidNumberFormat> {
    let mut segments = Vec::new();
    let mut text = String::new();
    let mut slots = 0;
    let mut chars = written.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            text.push(c);
            continue;
        }
        match chars.next() {
            Some('%') => text.push('%'),
            Some('s') => {
                slots += 1;
                push_text(&mut segments, &mut text);
                segments.push(Segment::Number);
            }
            _ => {
                return Err(InvalidNumberFormat::UnsupportedConversion(
                    written.to_string(),
                ));
            }
        }
    }
    match slots {
        0 => return Err(InvalidNumberFormat::NoNumberSlot(written.to_string())),
        1 => {}
        _ => return Err(InvalidNumberFormat::SeveralNumberSlots(written.to_string())),
    }
    push_text(&mut segments, &mut text);
    Ok(segments)
}

/// Moves accumulated literal text into `segments`, if there is any.
fn push_text(segments: &mut Vec<Segment>, text: &mut String) {
    if !text.is_empty() {
        segments.push(Segment::Text(std::mem::take(text)));
    }
}

impl Serialize for NumberFormat {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.written)
    }
}

impl<'de> Deserialize<'de> for NumberFormat {
    /// Re-parses on load, as every opaque type here does: a `.ast` or a
    /// `rinx.toml` is a file on disk, and nothing guarantees this build
    /// wrote it.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let written = String::deserialize(deserializer)?;
        Self::parse(&written).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn applied(written: &str, number: &str, name: Option<&str>) -> String {
        NumberFormat::parse(written)
            .expect("a valid format")
            .apply(number, name)
    }

    #[test]
    fn test_old_style_fills_the_number_into_percent_s() {
        // Given / When / Then
        assert_eq!(applied("Fig. %s", "1.2", None), "Fig. 1.2");
        assert_eq!(applied("%s here", "3", None), "3 here");
    }

    #[test]
    fn test_old_style_reads_a_doubled_percent_as_one() {
        // Given / When / Then
        assert_eq!(applied("100%% of %s", "4", None), "100% of 4");
    }

    #[test]
    fn test_old_style_refuses_a_format_without_a_slot() {
        // Given / When
        let result = NumberFormat::parse("see this");

        // Then — Python's `%` raises "not all arguments converted"
        assert_eq!(
            result,
            Err(InvalidNumberFormat::NoNumberSlot("see this".to_string()))
        );
    }

    #[test]
    fn test_old_style_refuses_two_slots() {
        // Given / When
        let result = NumberFormat::parse("a%sb%s");

        // Then
        assert_eq!(
            result,
            Err(InvalidNumberFormat::SeveralNumberSlots(
                "a%sb%s".to_string()
            ))
        );
    }

    #[test]
    fn test_old_style_refuses_another_conversion_and_a_trailing_percent() {
        // Given / When / Then
        assert!(matches!(
            NumberFormat::parse("Fig. %d"),
            Err(InvalidNumberFormat::UnsupportedConversion(_))
        ));
        assert!(matches!(
            NumberFormat::parse("%s %"),
            Err(InvalidNumberFormat::UnsupportedConversion(_))
        ));
    }

    #[test]
    fn test_new_style_fills_number_and_name() {
        // Given / When / Then
        assert_eq!(
            applied("Figure {number} ({name})", "1", Some("Root caption")),
            "Figure 1 (Root caption)"
        );
    }

    #[test]
    fn test_the_word_number_selects_new_style_as_in_sphinx() {
        // Given — no field at all, but the word Sphinx tests for
        // When / Then — shown as written, `%%` included
        assert_eq!(applied("only number", "1", None), "only number");
        assert_eq!(applied("100%% {number}", "1", None), "100%% 1");
    }

    #[test]
    fn test_new_style_reads_doubled_braces_as_literals() {
        // Given / When / Then
        assert_eq!(
            applied("{{number}} is {number}", "2", None),
            "{number} is 2"
        );
    }

    #[test]
    fn test_new_style_refuses_an_unknown_field() {
        // Given / When
        let result = NumberFormat::parse("{number} {bogus}");

        // Then
        assert_eq!(
            result,
            Err(InvalidNumberFormat::UnknownField {
                format: "{number} {bogus}".to_string(),
                field: "bogus".to_string(),
            })
        );
    }

    #[test]
    fn test_new_style_refuses_a_format_spec_by_its_whole_field() {
        // Given / When
        let result = NumberFormat::parse("{number:>3}");

        // Then
        assert!(matches!(
            result,
            Err(InvalidNumberFormat::UnknownField { field, .. }) if field == "number:>3"
        ));
    }

    #[test]
    fn test_new_style_refuses_unbalanced_braces() {
        // Given / When / Then
        assert!(matches!(
            NumberFormat::parse("{number"),
            Err(InvalidNumberFormat::UnbalancedBrace(_))
        ));
        assert!(matches!(
            NumberFormat::parse("number }"),
            Err(InvalidNumberFormat::UnbalancedBrace(_))
        ));
    }

    #[test]
    fn test_braces_in_an_old_style_format_are_literal() {
        // Given — neither `{name}` nor the word `number`, so `%` applies
        // When / Then
        assert_eq!(applied("{x} %s", "1", None), "{x} 1");
    }

    #[test]
    fn test_requires_name_only_for_a_name_field() {
        // Given / When / Then
        assert!(
            NumberFormat::parse("{name}")
                .expect("valid")
                .requires_name()
        );
        assert!(
            !NumberFormat::parse("Fig. {number}")
                .expect("valid")
                .requires_name()
        );
        assert!(
            !NumberFormat::parse("Fig. %s")
                .expect("valid")
                .requires_name()
        );
    }

    #[test]
    fn test_as_str_is_the_text_as_written() {
        // Given / When
        let format = NumberFormat::parse("Fig. %s").expect("valid");

        // Then
        assert_eq!(format.as_str(), "Fig. %s");
    }

    #[test]
    fn test_push_text_skips_empty_text() {
        // Given
        let mut segments = Vec::new();
        let mut text = String::new();

        // When
        push_text(&mut segments, &mut text);

        // Then
        assert!(segments.is_empty());
    }

    #[test]
    fn test_round_trips_through_json_and_revalidates_on_load() {
        // Given
        let format = NumberFormat::parse("Tbl. %s").expect("valid");

        // When
        let json = serde_json::to_string(&format).expect("serializes");
        let restored: NumberFormat = serde_json::from_str(&json).expect("deserializes");
        let refused = serde_json::from_str::<NumberFormat>("\"no slot\"");

        // Then
        assert_eq!(json, "\"Tbl. %s\"");
        assert_eq!(restored, format);
        assert!(refused.is_err());
    }

    #[test]
    fn test_messages_name_the_format_and_the_fix() {
        // Given / When
        let message = NumberFormat::parse("see this").unwrap_err().to_string();

        // Then
        assert!(message.contains("'see this'"), "{message}");
        assert!(message.contains("%s or {number}"), "{message}");
    }
}
