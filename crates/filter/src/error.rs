use std::fmt;

/// Why a filter expression could not be parsed.
///
/// Carries a **character** range into the filter text, not a byte one: the
/// caller turns it into a column, and a column is counted in characters
/// everywhere in this build. The range is half-open, `offset..offset + length`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterError {
    pub kind: FilterErrorKind,
    /// Characters from the start of the filter text to the offending token.
    pub offset: usize,
    /// The offending token's length in characters; never zero, so a caret can
    /// always cover something. At the end of the input it covers the last
    /// character.
    pub length: usize,
}

impl FilterError {
    /// Builds an error covering `length` characters from `offset`.
    #[must_use]
    pub(crate) fn new(kind: FilterErrorKind, offset: usize, length: usize) -> Self {
        Self {
            kind,
            offset,
            length: length.max(1),
        }
    }
}

impl fmt::Display for FilterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}

impl std::error::Error for FilterError {}

/// What went wrong, separately from where.
///
/// Most variants name a *construct* rather than describing a token soup.
/// Sphinx-needs filters are Python expressions and this language evaluates a
/// subset of them, so the common failure is not a typo but a construct we
/// deliberately do not support — and being told which one is the difference
/// between a fixable message and "syntax error".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterErrorKind {
    /// The expression ended while something was still expected.
    UnexpectedEnd,
    /// A token appeared where the grammar allows none.
    UnexpectedToken(String),
    /// A string literal was opened and never closed.
    UnterminatedString,
    /// A number was written that does not fit an `i64`.
    NumberOutOfRange(String),
    /// A field name that does not fit [`FieldName`](crate::FieldName)'s rules.
    IllegalFieldName(String),
    /// A Python construct this language deliberately does not evaluate.
    ///
    /// `hint` is what to write instead, when there is an equivalent.
    Unsupported {
        construct: &'static str,
        hint: Option<&'static str>,
    },
}

impl FilterErrorKind {
    /// An unsupported construct with an alternative to suggest.
    pub(crate) fn unsupported_but(construct: &'static str, hint: &'static str) -> Self {
        Self::Unsupported {
            construct,
            hint: Some(hint),
        }
    }

    /// An unsupported construct with no equivalent in this language.
    pub(crate) fn unsupported(construct: &'static str) -> Self {
        Self::Unsupported {
            construct,
            hint: None,
        }
    }
}

impl fmt::Display for FilterErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEnd => write!(f, "the expression ends too early"),
            Self::UnexpectedToken(text) => write!(f, "unexpected {text}"),
            Self::UnterminatedString => write!(f, "unterminated string"),
            Self::NumberOutOfRange(text) => write!(f, "the number {text} is too large"),
            Self::IllegalFieldName(reason) => write!(f, "{reason}"),
            Self::Unsupported {
                construct,
                hint: Some(hint),
            } => write!(f, "{construct} are not supported here; {hint}"),
            Self::Unsupported {
                construct,
                hint: None,
            } => write!(f, "{construct} are not supported here"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_an_error_never_covers_zero_characters() {
        // Given — an empty span would leave a caret with nothing to point at
        let error = FilterError::new(FilterErrorKind::UnexpectedEnd, 4, 0);

        // When
        let length = error.length;

        // Then
        assert_eq!(length, 1);
    }

    #[test]
    fn test_an_error_keeps_the_offset_it_was_given() {
        // Given
        let error = FilterError::new(FilterErrorKind::UnexpectedEnd, 7, 3);

        // When
        let (offset, length) = (error.offset, error.length);

        // Then
        assert_eq!((offset, length), (7, 3));
    }

    #[test]
    fn test_an_unsupported_construct_with_a_hint_names_the_alternative() {
        // Given
        let kind =
            FilterErrorKind::unsupported_but("comparisons with `!=` to None", "use `is not None`");

        // When
        let message = kind.to_string();

        // Then
        assert_eq!(
            message,
            "comparisons with `!=` to None are not supported here; use `is not None`"
        );
    }

    #[test]
    fn test_an_unsupported_construct_without_a_hint_just_names_it() {
        // Given
        let kind = FilterErrorKind::unsupported("function calls");

        // When
        let message = kind.to_string();

        // Then
        assert_eq!(message, "function calls are not supported here");
    }

    #[test]
    fn test_every_kind_describes_itself() {
        // Given
        let kinds = [
            FilterErrorKind::UnexpectedEnd,
            FilterErrorKind::UnexpectedToken("`)`".to_string()),
            FilterErrorKind::UnterminatedString,
            FilterErrorKind::NumberOutOfRange("999...".to_string()),
            FilterErrorKind::IllegalFieldName("field name is empty".to_string()),
        ];

        // When
        let messages: Vec<String> = kinds.iter().map(ToString::to_string).collect();

        // Then
        assert!(messages.iter().all(|message| !message.is_empty()));
        assert!(messages[1].contains("unexpected"));
        assert!(messages[2].contains("unterminated"));
    }

    #[test]
    fn test_the_error_displays_its_kind() {
        // Given — the caller formats position separately, so the message must
        // not repeat it
        let error = FilterError::new(FilterErrorKind::UnterminatedString, 12, 1);

        // When
        let message = error.to_string();

        // Then
        assert_eq!(message, "unterminated string");
    }
}
