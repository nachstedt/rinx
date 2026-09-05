use serde::{Deserialize, Serialize};

use crate::node::Node;

/// One entry in an RST option list: one or more comma-separated option
/// synonyms (`-h, --help`) followed by a description.
///
/// Unlike [`DefinitionListItem`](crate::DefinitionListItem), the "term" side
/// is not free-form inline text — it's a fixed grammar of option markers
/// (see [`OptionSpec`]) that docutils itself defines, so it is modeled
/// structurally rather than as parsed inline markup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionListItem {
    pub options: Vec<OptionSpec>,
    pub description: Vec<Node>,
}

/// One option synonym within an [`OptionListItem`], e.g. `-o`, `--output`,
/// `/Wall`, `+x`, optionally followed by an argument placeholder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionSpec {
    /// The flag itself, sigil included: `"-o"`, `"--output"`, `"/Wall"`,
    /// `"+x"`.
    pub flag: String,
    pub argument: Option<OptionArgument>,
}

/// An option's argument placeholder, e.g. the `FILE` in `-o FILE` or
/// `--output=FILE`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionArgument {
    /// The placeholder text, e.g. `"FILE"` or `"<value1 value2>"` (a
    /// bracketed argument keeps its brackets).
    pub text: String,
    pub delimiter: OptionArgumentDelimiter,
}

/// How an [`OptionArgument`] is joined to its flag when rendered, mirroring
/// docutils' own `parse_option_marker`, which records exactly this so the
/// HTML writer can reproduce the author's original spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptionArgumentDelimiter {
    /// `-o FILE`, `--output FILE`.
    Space,
    /// `--output=FILE`. Long options only.
    Equals,
    /// `-oFILE`. Short options only.
    Adjacent,
}

impl OptionArgumentDelimiter {
    /// The literal text rendered between a flag and its argument.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Space => " ",
            Self::Equals => "=",
            Self::Adjacent => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_option_argument_delimiter_space_renders_as_single_space() {
        // Given / When / Then
        assert_eq!(OptionArgumentDelimiter::Space.as_str(), " ");
    }

    #[test]
    fn test_option_argument_delimiter_equals_renders_as_equals_sign() {
        // Given / When / Then
        assert_eq!(OptionArgumentDelimiter::Equals.as_str(), "=");
    }

    #[test]
    fn test_option_argument_delimiter_adjacent_renders_as_empty_string() {
        // Given / When / Then
        assert_eq!(OptionArgumentDelimiter::Adjacent.as_str(), "");
    }

    #[test]
    fn test_option_list_item_serialization_roundtrip() {
        // Given
        let item = OptionListItem {
            options: vec![
                OptionSpec {
                    flag: "-o".to_string(),
                    argument: Some(OptionArgument {
                        text: "FILE".to_string(),
                        delimiter: OptionArgumentDelimiter::Space,
                    }),
                },
                OptionSpec {
                    flag: "--output".to_string(),
                    argument: Some(OptionArgument {
                        text: "FILE".to_string(),
                        delimiter: OptionArgumentDelimiter::Equals,
                    }),
                },
            ],
            description: vec![Node::Paragraph(vec![])],
        };

        // When
        let json = serde_json::to_string(&item).expect("Failed to serialize");
        let deserialized: OptionListItem =
            serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(item, deserialized);
    }
}
