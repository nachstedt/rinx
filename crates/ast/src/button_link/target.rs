//! Where a button points.
//!
//! One variant today, which is deliberate. sphinx-design builds two directives
//! from one base class: `.. button-link::`, whose argument is a URL, and
//! `.. button-ref::`, whose argument is a cross-reference target resolved
//! against the project. They differ in nothing else — same options, same
//! rendered element, same classes — so the difference is *this* type and not
//! a second node.
//!
//! A `Reference` variant is the slot the second directive takes. Keeping the
//! enum here from the start means adding it changes no `match` that already
//! exists and no `.ast` file that was already written; the alternative, a bare
//! `String` widened later, would break both.

use serde::{Deserialize, Serialize};

/// The destination a `.. button-link::` sends its reader to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ButtonTarget {
    /// An external URL, written as the directive's argument.
    ///
    /// Stored with every whitespace character removed, as docutils'
    /// `directives.uri` normalizes one — a URL split across two argument lines
    /// is one URL, not two words.
    Url(String),
}

impl ButtonTarget {
    /// The text this target is shown as when the directive wrote no label.
    ///
    /// sphinx-design falls back to the target itself rather than rendering an
    /// empty button, which is what makes a bare `.. button-link:: <url>` a
    /// usable directive. Kept on the type so the parser and the renderer
    /// cannot disagree about what that fallback is.
    #[must_use]
    pub fn display_text(&self) -> &str {
        match self {
            Self::Url(url) => url,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_text_of_a_url_target_is_the_url() {
        // Given
        let target = ButtonTarget::Url("https://example.com/docs".to_string());

        // When
        let text = target.display_text();

        // Then
        assert_eq!(text, "https://example.com/docs");
    }

    #[test]
    fn test_a_target_round_trips_through_serialization() {
        // Given
        let target = ButtonTarget::Url("https://example.com".to_string());

        // When
        let json = serde_json::to_string(&target).expect("target should serialize");
        let restored: ButtonTarget =
            serde_json::from_str(&json).expect("target should deserialize");

        // Then
        assert_eq!(restored, target);
    }
}
