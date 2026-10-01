//! The target shape `:pep:`, `:rfc:` and `:cwe:` share: a number, then an
//! optional `#` and anchor — `8`, `2324#section-2.3`.
//!
//! Sphinx splits each of these roles' text at its first `#` and hands the
//! part before it to Python's `int()`, refusing the role when that raises;
//! [`NumberedTarget`] is that split and that refusal, made where the text is
//! read. Only ASCII digits are a number here: `int()` also accepts a sign, an
//! underscore between digits and non-ASCII digits — none of which names a
//! document anybody wrote — so those are refused rather than reproduced.

use super::invalid::{InvalidRegistryTarget, InvalidRegistryTargetReason};
use super::registry::Registry;

/// A number with an optional fragment, as a registry role wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NumberedTarget {
    /// The text as written, kept because Sphinx shows it: ``:pep:`08` ``
    /// reads "PEP 08", fragment and all.
    written: String,
    number: u32,
    /// Where the fragment starts in `written`, after its `#`.
    fragment_start: Option<usize>,
}

impl NumberedTarget {
    /// Reads a written target for `registry`, splitting off a fragment at
    /// the first `#`.
    pub(super) fn parse(registry: Registry, written: &str) -> Result<Self, InvalidRegistryTarget> {
        let (digits, fragment_start) = split_fragment(written);
        let number = read_ascii_number(digits).map_err(|error| {
            let reason = match error {
                NumberError::NotDigits => InvalidRegistryTargetReason::NotANumber,
                NumberError::TooLarge => InvalidRegistryTargetReason::TooLarge,
            };
            InvalidRegistryTarget::new(registry, written, reason)
        })?;
        Ok(Self {
            written: written.to_string(),
            number,
            fragment_start,
        })
    }

    pub(super) fn as_written(&self) -> &str {
        &self.written
    }

    pub(super) const fn number(&self) -> u32 {
        self.number
    }

    /// The number as written, before any `#`.
    pub(super) fn written_number(&self) -> &str {
        self.fragment_start
            .map_or(self.written.as_str(), |start| &self.written[..start - 1])
    }

    /// The anchor after the `#`, which may be empty; `None` without a `#`.
    pub(super) fn fragment(&self) -> Option<&str> {
        self.fragment_start.map(|start| &self.written[start..])
    }

    /// `page` followed by `#` and the fragment, when one was written — even
    /// an empty one, as Sphinx appends the `#` whenever the target has one.
    pub(super) fn with_fragment(&self, page: String) -> String {
        match self.fragment() {
            Some(fragment) => format!("{page}#{fragment}"),
            None => page,
        }
    }
}

/// Splits `written` at its first `#`: the part before it, and where the
/// fragment starts in `written`.
pub(super) fn split_fragment(written: &str) -> (&str, Option<usize>) {
    match written.split_once('#') {
        Some((before, _)) => (before, Some(before.len() + 1)),
        None => (written, None),
    }
}

/// Why [`read_ascii_number`] refused a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumberError {
    /// Empty, or holding anything but ASCII digits.
    NotDigits,
    /// A run of digits too long for a `u32`.
    TooLarge,
}

/// Reads a number written as a run of ASCII digits — the one spelling of a
/// number every registry role and docutils' `:pep-reference:` and
/// `:rfc-reference:` accept, kept in one place so no two can disagree about
/// what a number is.
pub(crate) fn read_ascii_number(digits: &str) -> Result<u32, NumberError> {
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(NumberError::NotDigits);
    }
    digits.parse().map_err(|_| NumberError::TooLarge)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_reads_a_bare_number() {
        // Given / When
        let target = NumberedTarget::parse(Registry::Rfc, "0042").unwrap();

        // Then
        assert_eq!(target.number(), 42);
        assert_eq!(target.as_written(), "0042");
        assert_eq!(target.written_number(), "0042");
        assert_eq!(target.fragment(), None);
    }

    #[test]
    fn test_parse_splits_the_fragment_at_the_first_hash() {
        // Given / When
        let target = NumberedTarget::parse(Registry::Pep, "8#a#b").unwrap();

        // Then
        assert_eq!(target.number(), 8);
        assert_eq!(target.written_number(), "8");
        assert_eq!(target.fragment(), Some("a#b"));
    }

    #[test]
    fn test_parse_refuses_what_is_not_digits_naming_the_registry() {
        // Given / When / Then
        for written in ["", "#a", "abc", "+8", "-1", "8_0", " 8", "٨"] {
            let error = NumberedTarget::parse(Registry::Cwe, written).unwrap_err();
            assert_eq!(error.registry(), Registry::Cwe, "{written:?}");
            assert_eq!(error.target(), written);
            assert_eq!(error.reason(), InvalidRegistryTargetReason::NotANumber);
        }
    }

    #[test]
    fn test_parse_refuses_a_number_too_large() {
        // Given / When
        let error = NumberedTarget::parse(Registry::Pep, "99999999999").unwrap_err();

        // Then
        assert_eq!(error.reason(), InvalidRegistryTargetReason::TooLarge);
    }

    #[test]
    fn test_with_fragment_appends_a_written_fragment_even_an_empty_one() {
        // Given / When / Then
        let plain = NumberedTarget::parse(Registry::Rfc, "1").unwrap();
        let anchored = NumberedTarget::parse(Registry::Rfc, "1#s").unwrap();
        let empty = NumberedTarget::parse(Registry::Rfc, "1#").unwrap();
        assert_eq!(plain.with_fragment("p".to_string()), "p");
        assert_eq!(anchored.with_fragment("p".to_string()), "p#s");
        assert_eq!(empty.with_fragment("p".to_string()), "p#");
    }

    #[test]
    fn test_split_fragment_finds_the_first_hash() {
        // Given / When / Then
        assert_eq!(split_fragment("a#b#c"), ("a", Some(2)));
        assert_eq!(split_fragment("abc"), ("abc", None));
    }

    #[test]
    fn test_read_ascii_number_reads_ascii_digits() {
        // Given / When / Then
        assert_eq!(read_ascii_number("0008"), Ok(8));
        assert_eq!(read_ascii_number("0"), Ok(0));
    }

    #[test]
    fn test_read_ascii_number_refuses_anything_else() {
        // Given / When / Then
        for digits in ["", "+8", "8 ", "8#a", "٨"] {
            assert_eq!(
                read_ascii_number(digits),
                Err(NumberError::NotDigits),
                "{digits:?}"
            );
        }
        assert_eq!(read_ascii_number("99999999999"), Err(NumberError::TooLarge));
    }
}
