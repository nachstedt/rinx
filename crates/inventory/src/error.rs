//! What can go wrong reading an inventory file.

/// Why a file could not be read as an inventory at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryError {
    /// The first line is not a Sphinx inventory header.
    NotAnInventory,
    /// A version 1 inventory — uncompressed, and without the display name and
    /// priority columns. Refused by name rather than as a generic header
    /// error: Sphinx stopped writing it in 2010, but it is a real format, so
    /// the file is not *wrong*, only too old.
    UnsupportedVersion1,
    /// A header line other than the first is missing or not what Sphinx
    /// writes; `expected` names the line.
    MalformedHeader { expected: &'static str },
    /// The body after the header is not valid zlib data.
    Decompression(String),
    /// The decompressed body is not UTF-8.
    NotUtf8,
}

impl std::fmt::Display for InventoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAnInventory => f.write_str(
                "not a Sphinx inventory: the first line must be `# Sphinx inventory version 2`",
            ),
            Self::UnsupportedVersion1 => f.write_str(
                "a version 1 Sphinx inventory is not supported; regenerate it with any Sphinx \
                 release since 1.0, which writes version 2",
            ),
            Self::MalformedHeader { expected } => {
                write!(
                    f,
                    "malformed inventory header: expected the {expected} line"
                )
            }
            Self::Decompression(message) => {
                write!(f, "the inventory body is not valid zlib data: {message}")
            }
            Self::NotUtf8 => f.write_str("the inventory body is not valid UTF-8"),
        }
    }
}

impl std::error::Error for InventoryError {}

/// A body line Sphinx's own reader would silently skip.
///
/// Reported rather than skipped here — an entry that vanishes without a word
/// is a broken link nobody can explain — but not an error either, since the
/// rest of the file is still good and Sphinx itself would use it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MalformedLine {
    /// 1-based, counting the header's four lines, so it matches what a
    /// decompressed view of the file shows.
    pub line: usize,
    pub text: String,
    pub reason: &'static str,
}

impl std::fmt::Display for MalformedLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}: `{}`", self.line, self.reason, self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_1_message_says_how_to_fix_it() {
        // Given
        let error = InventoryError::UnsupportedVersion1;

        // When
        let message = error.to_string();

        // Then
        assert!(message.contains("version 1"));
        assert!(message.contains("regenerate"));
    }

    #[test]
    fn test_malformed_header_message_names_the_line() {
        // Given
        let error = InventoryError::MalformedHeader {
            expected: "`# Project:`",
        };

        // When
        let message = error.to_string();

        // Then
        assert!(message.contains("`# Project:`"));
    }

    #[test]
    fn test_every_error_has_a_message() {
        // Given
        let errors = [
            InventoryError::NotAnInventory,
            InventoryError::Decompression("bad".to_string()),
            InventoryError::NotUtf8,
        ];

        // When / Then
        for error in errors {
            assert!(!error.to_string().is_empty());
        }
    }

    #[test]
    fn test_malformed_line_display_quotes_line_and_reason() {
        // Given
        let malformed = MalformedLine {
            line: 7,
            text: "garbage".to_string(),
            reason: "not five fields",
        };

        // When
        let message = malformed.to_string();

        // Then
        assert_eq!(message, "line 7: not five fields: `garbage`");
    }
}
