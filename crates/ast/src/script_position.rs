//! Whether an [`crate::InlineNode::Script`] sits below or above the line: the
//! one thing docutils' `:sub:`/`:subscript:` and `:sup:`/`:superscript:` roles
//! differ in.
//!
//! The four role names are listed here and nowhere else, so the inline scan
//! that reads a role and the `.. role::` directive that derives one from it
//! cannot disagree about which spellings exist.

use serde::{Deserialize, Serialize};

/// Below the line (`<sub>`) or above it (`<sup>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptPosition {
    Subscript,
    Superscript,
}

impl ScriptPosition {
    /// The position the role `name` sets, or `None` when `name` is none of
    /// the four. Case-sensitive, as every built-in role name here is.
    #[must_use]
    pub fn from_role_name(name: &str) -> Option<Self> {
        match name {
            "sub" | "subscript" => Some(Self::Subscript),
            "sup" | "superscript" => Some(Self::Superscript),
            _ => None,
        }
    }

    /// The HTML element the text is wrapped in.
    #[must_use]
    pub fn html_tag(self) -> &'static str {
        match self {
            Self::Subscript => "sub",
            Self::Superscript => "sup",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_role_name_accepts_both_spellings_of_each_role() {
        // Given / When / Then
        assert_eq!(
            ScriptPosition::from_role_name("sub"),
            Some(ScriptPosition::Subscript)
        );
        assert_eq!(
            ScriptPosition::from_role_name("subscript"),
            Some(ScriptPosition::Subscript)
        );
        assert_eq!(
            ScriptPosition::from_role_name("sup"),
            Some(ScriptPosition::Superscript)
        );
        assert_eq!(
            ScriptPosition::from_role_name("superscript"),
            Some(ScriptPosition::Superscript)
        );
    }

    #[test]
    fn test_from_role_name_refuses_other_names_and_other_cases() {
        // Given / When / Then
        assert_eq!(ScriptPosition::from_role_name("code"), None);
        assert_eq!(ScriptPosition::from_role_name("Sub"), None);
        assert_eq!(ScriptPosition::from_role_name(""), None);
    }

    #[test]
    fn test_html_tag_names_the_element() {
        // Given / When / Then
        assert_eq!(ScriptPosition::Subscript.html_tag(), "sub");
        assert_eq!(ScriptPosition::Superscript.html_tag(), "sup");
    }

    #[test]
    fn test_serde_round_trip() {
        // Given
        let position = ScriptPosition::Superscript;

        // When
        let json = serde_json::to_string(&position).unwrap();
        let back: ScriptPosition = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(json, "\"superscript\"");
        assert_eq!(back, position);
    }
}
