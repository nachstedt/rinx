//! The `.. dropdown::` node itself: its title, its ten options and its body.

use serde::{Deserialize, Serialize};

use crate::dropdown::animation::Animation;
use crate::dropdown::chevron::Chevron;
use crate::dropdown::color::SemanticColor;
use crate::dropdown::octicon::OcticonName;
use crate::inline_node::InlineNode;
use crate::node::Node;
use crate::spacing::Spacing;
use crate::span::Span;
use crate::target_name::TargetName;

/// A collapsible container — sphinx-design's `.. dropdown::`.
///
/// The title is inline markup rather than a plain string, unlike the title an
/// admonition carries: sphinx-design runs the directive's argument through
/// `inline_text`, so `` .. dropdown:: See ``config.toml`` `` really does hold
/// a literal. An empty `title` means no argument was written at all, which is
/// a rendered difference rather than an empty heading — see the renderer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dropdown {
    /// The directive argument, parsed as inline markup. Empty when none was
    /// written.
    pub title: Vec<InlineNode>,
    /// `:open:` — whether the `<details>` starts expanded.
    pub open: bool,
    /// `:color:` — the semantic colour of the summary bar.
    pub color: Option<SemanticColor>,
    /// `:icon:` — the octicon shown before the title. A *name*, checked
    /// against the icon set while parsing rather than by this type — see
    /// [`OcticonName`].
    pub icon: Option<OcticonName>,
    /// `:chevron:` — which state marker to draw. Not optional: an omitted
    /// `:chevron:` means [`Chevron::RightDown`], which is a real choice
    /// rather than an absent one.
    pub chevron: Chevron,
    /// `:animate:` — how the body appears when the dropdown opens.
    pub animate: Option<Animation>,
    /// `:margin:` — the space kept around the container. `None` renders
    /// [`Dropdown::DEFAULT_MARGIN_CLASSES`], not nothing.
    pub margin: Option<Spacing>,
    /// `:name:` — reuses [`TargetName`] like the image, table and math
    /// directives do, so it registers in `ProjectIndex::targets` with no
    /// conversion.
    pub name: Option<TargetName>,
    /// `:class-container:` — extra classes for the `<details>`.
    pub class_container: Vec<String>,
    /// `:class-title:` — extra classes for the `<summary>`.
    pub class_title: Vec<String>,
    /// `:class-body:` — extra classes for the content `<div>`.
    pub class_body: Vec<String>,
    /// The directive's content, parsed as ordinary block content.
    pub body: Vec<Node>,
    /// Where the directive was written, carried for the same reason
    /// [`crate::ImageOptions`] carries one: so a later phase reporting
    /// anything about this dropdown has a position to report it against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

impl Dropdown {
    /// The classes an *omitted* `:margin:` produces.
    ///
    /// sphinx-design defaults the container's margin classes to `sd-mb-3`
    /// rather than to nothing, so a dropdown with no `:margin:` still clears
    /// the content below it. Kept here, beside the node it belongs to, rather
    /// than on the shared [`Spacing`] — the default is the *directive's*, not
    /// the spacing vocabulary's, and `.. grid::` has a different one.
    pub const DEFAULT_MARGIN_CLASSES: &'static [&'static str] = &["sd-mb-3"];

    /// A dropdown with no title, no options and no content.
    #[must_use]
    pub fn new() -> Self {
        Self {
            title: Vec::new(),
            open: false,
            color: None,
            icon: None,
            chevron: Chevron::default(),
            animate: None,
            margin: None,
            name: None,
            class_container: Vec::new(),
            class_title: Vec::new(),
            class_body: Vec::new(),
            body: Vec::new(),
            span: None,
        }
    }

    /// Whether an argument was written.
    ///
    /// sphinx-design keeps this as its own `has_title` flag because a
    /// titleless dropdown does not render an empty summary — it renders a
    /// placeholder icon instead.
    #[must_use]
    pub fn has_title(&self) -> bool {
        !self.title.is_empty()
    }
}

impl Default for Dropdown {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spacing::SpacingValue;

    #[test]
    fn test_default_margin_classes_match_the_bottom_margin_sphinx_design_applies() {
        // Given / When / Then
        assert_eq!(Dropdown::DEFAULT_MARGIN_CLASSES, &["sd-mb-3"]);
    }

    #[test]
    fn test_new_is_closed_with_no_options() {
        // Given / When
        let dropdown = Dropdown::new();

        // Then
        assert!(!dropdown.open);
        assert_eq!(dropdown.color, None);
        assert_eq!(dropdown.icon, None);
        assert_eq!(dropdown.animate, None);
        assert_eq!(dropdown.margin, None);
        assert_eq!(dropdown.name, None);
        assert!(dropdown.class_container.is_empty());
        assert!(dropdown.class_title.is_empty());
        assert!(dropdown.class_body.is_empty());
        assert!(dropdown.body.is_empty());
        assert_eq!(dropdown.span, None);
    }

    #[test]
    fn test_new_defaults_the_chevron_rather_than_leaving_it_unset() {
        // Given / When
        let dropdown = Dropdown::new();

        // Then
        assert_eq!(dropdown.chevron, Chevron::RightDown);
    }

    #[test]
    fn test_default_matches_new() {
        // Given / When / Then
        assert_eq!(Dropdown::default(), Dropdown::new());
    }

    #[test]
    fn test_has_title_is_false_without_an_argument() {
        // Given
        let dropdown = Dropdown::new();

        // When / Then
        assert!(!dropdown.has_title());
    }

    #[test]
    fn test_has_title_is_true_once_an_argument_was_parsed() {
        // Given
        let dropdown = Dropdown {
            title: vec![InlineNode::Text("Details".to_string())],
            ..Dropdown::new()
        };

        // When / Then
        assert!(dropdown.has_title());
    }

    #[test]
    fn test_round_trips_through_json() {
        // Given — every option set, so no field is silently dropped
        let dropdown = Dropdown {
            title: vec![InlineNode::Text("Details".to_string())],
            open: true,
            color: Some(SemanticColor::Success),
            icon: Some(OcticonName::new("light-bulb").expect("a slug is valid")),
            chevron: Chevron::DownUp,
            animate: Some(Animation::FadeIn),
            margin: Some(Spacing::All(SpacingValue::Three)),
            name: Some(TargetName::new("my-dropdown")),
            class_container: vec!["a".to_string()],
            class_title: vec!["b".to_string()],
            class_body: vec!["c".to_string()],
            body: vec![Node::Paragraph(vec![InlineNode::Text("hi".to_string())])],
            span: None,
        };

        // When
        let json = serde_json::to_string(&dropdown).expect("serializing cannot fail");
        let restored: Dropdown = serde_json::from_str(&json).expect("a written dropdown reloads");

        // Then
        assert_eq!(restored, dropdown);
    }
}
