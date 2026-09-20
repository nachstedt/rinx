//! The `.. button-link::` node itself: its target, its label and its nine
//! options.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::button_link::flag::ButtonFlag;
use crate::button_link::target::ButtonTarget;
use crate::button_link::text_align::TextAlign;
use crate::dropdown::SemanticColor;
use crate::inline_node::InlineNode;
use crate::span::Span;

/// A button-shaped link — sphinx-design's `.. button-link::`.
///
/// The label is inline markup rather than a plain string, like a
/// [`crate::Dropdown`]'s title and unlike every other caption in this build:
/// sphinx-design runs the directive's *content* through `inline_text`, so a
/// role or a literal in the label really is one. An empty `label` means no
/// content was written, which renders the target itself rather than an empty
/// button — see [`Self::has_label`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ButtonLink {
    /// Where the button points — the directive's argument.
    pub target: ButtonTarget,
    /// The directive's content, parsed as inline markup. Empty when none was
    /// written.
    pub label: Vec<InlineNode>,
    /// `:color:` — the semantic colour the button is painted in. `None`
    /// paints nothing: sphinx-design adds a colour class only when the option
    /// was written.
    pub color: Option<SemanticColor>,
    /// The value-less options that were written — see [`ButtonFlag`].
    pub flags: BTreeSet<ButtonFlag>,
    /// `:align:` — where the button sits in the line it occupies. Carried by
    /// the *containing* element rather than the button itself.
    pub align: Option<TextAlign>,
    /// `:tooltip:` — the native tooltip, rendered as the `title` attribute.
    pub tooltip: Option<String>,
    /// `:class:` — extra classes for the button itself.
    pub class: Vec<String>,
    /// Where the directive was written, carried for the same reason
    /// [`crate::ImageOptions`] carries one: so a later phase reporting
    /// anything about this button has a position to report it against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

impl ButtonLink {
    /// A button pointing at `target`, with no label and no options.
    #[must_use]
    pub fn new(target: ButtonTarget) -> Self {
        Self {
            target,
            label: Vec::new(),
            color: None,
            flags: BTreeSet::new(),
            align: None,
            tooltip: None,
            class: Vec::new(),
            span: None,
        }
    }

    /// Whether any content was written.
    ///
    /// sphinx-design keeps this distinction because a label-less button is not
    /// an empty one: it falls back to showing the target, which is what makes
    /// a bare `.. button-link:: <url>` worth writing.
    #[must_use]
    pub fn has_label(&self) -> bool {
        !self.label.is_empty()
    }

    /// Whether this option was written.
    #[must_use]
    pub fn has(&self, flag: ButtonFlag) -> bool {
        self.flags.contains(&flag)
    }

    /// Whether `:outline:` was written with no `:color:` to outline.
    ///
    /// sphinx-design produces no class at all for that combination, so the
    /// option silently does nothing. Kept here rather than in the parser
    /// because it is a property of the node's own fields, and the renderer's
    /// class list is written against the same rule.
    #[must_use]
    pub fn has_unusable_outline(&self) -> bool {
        self.has(ButtonFlag::Outline) && self.color.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn button() -> ButtonLink {
        ButtonLink::new(ButtonTarget::Url("https://example.com".to_string()))
    }

    #[test]
    fn test_a_new_button_has_its_target_and_nothing_else() {
        // Given / When
        let link = button();

        // Then
        assert_eq!(
            link.target,
            ButtonTarget::Url("https://example.com".to_string())
        );
        assert!(link.label.is_empty());
        assert_eq!(link.color, None);
        assert!(link.flags.is_empty());
        assert_eq!(link.align, None);
        assert_eq!(link.tooltip, None);
        assert!(link.class.is_empty());
        assert_eq!(link.span, None);
    }

    #[test]
    fn test_has_label_distinguishes_written_content_from_none() {
        // Given
        let empty = button();
        let mut labelled = button();
        labelled.label = vec![InlineNode::Text("Read the docs".to_string())];

        // When / Then
        assert!(!empty.has_label());
        assert!(labelled.has_label());
    }

    #[test]
    fn test_outline_without_a_color_is_unusable() {
        // Given
        let mut link = button();
        link.flags.insert(ButtonFlag::Outline);

        // When / Then
        assert!(link.has_unusable_outline());
    }

    #[test]
    fn test_outline_with_a_color_is_usable() {
        // Given
        let mut link = button();
        link.flags.insert(ButtonFlag::Outline);
        link.color = Some(SemanticColor::Primary);

        // When / Then
        assert!(!link.has_unusable_outline());
    }

    #[test]
    fn test_a_color_without_outline_is_not_reported_as_unusable() {
        // Given
        let mut link = button();
        link.color = Some(SemanticColor::Primary);

        // When / Then
        assert!(!link.has_unusable_outline());
    }

    #[test]
    fn test_has_answers_only_for_the_flags_that_were_written() {
        // Given
        let mut link = button();
        link.flags.insert(ButtonFlag::Shadow);

        // When / Then
        assert!(link.has(ButtonFlag::Shadow));
        assert!(!link.has(ButtonFlag::Expand));
        assert!(!link.has(ButtonFlag::Outline));
        assert!(!link.has(ButtonFlag::ClickParent));
    }

    #[test]
    fn test_a_button_round_trips_through_serialization() {
        // Given
        let mut link = button();
        link.label = vec![InlineNode::Text("Open".to_string())];
        link.color = Some(SemanticColor::Success);
        link.flags.insert(ButtonFlag::Shadow);
        link.tooltip = Some("Opens the editor".to_string());
        link.class = vec!["my-button".to_string()];

        // When
        let json = serde_json::to_string(&link).expect("button should serialize");
        let restored: ButtonLink = serde_json::from_str(&json).expect("button should deserialize");

        // Then
        assert_eq!(restored, link);
    }
}
