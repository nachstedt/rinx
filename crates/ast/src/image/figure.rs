//! `.. figure::` — an image, a caption naming it, and a legend explaining it.
//!
//! Its own struct rather than a set of fields on [`crate::Directive`] because
//! a figure is an image *plus* four things, and carrying the sum inline would
//! make every `Node` in the document pay for it: an enum is as large as its
//! largest variant, and `Node::Directive` is already the largest node. Both
//! image directives are therefore boxed newtype variants, which is also the
//! shape the other closed directive families already use
//! ([`crate::DomainObjectBody`], [`crate::CodeBlock`], [`crate::DocTestBlock`]).

use serde::{Deserialize, Serialize};

use crate::inline_node::InlineNode;
use crate::node::Node;

use super::length::FigureWidth;
use super::options::ImageOptions;

/// The body of a `.. figure::` directive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Figure {
    /// The nine options an `.. image::` would take, identical here — the
    /// directive's argument and everything sizing the picture itself.
    pub image: ImageOptions,
    /// `:figwidth:` — how wide the figure *box* is, as opposed to the image
    /// inside it, which `image.width` sizes.
    pub figwidth: Option<FigureWidth>,
    /// `:figclass:` — space-separated class names for the figure box, already
    /// split. Separate from `image.classes`, which land on the `<img>` itself.
    pub figclasses: Vec<String>,
    /// The body's first paragraph, if it had one — parsed as inline markup
    /// rather than kept as text, since a caption routinely names things with
    /// roles. `None` when the body opened with an empty comment (`..`), which
    /// is docutils' way of writing a legend with no caption, and when the body
    /// was empty.
    pub caption: Option<Vec<InlineNode>>,
    /// Whatever followed the caption: arbitrary body content.
    pub legend: Vec<Node>,
}

impl Figure {
    /// A figure showing `image` with nothing written around it.
    #[must_use]
    pub fn new(image: ImageOptions) -> Self {
        Self {
            image,
            figwidth: None,
            figclasses: Vec::new(),
            caption: None,
            legend: Vec::new(),
        }
    }

    /// Whether anything was written below the directive's options.
    ///
    /// A figure with neither is legal and renders as a bare picture in a
    /// `<figure>` box; docutils does not treat it as an error.
    #[must_use]
    pub fn has_body(&self) -> bool {
        self.caption.is_some() || !self.legend.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AssetUri;

    fn bare_figure() -> Figure {
        Figure::new(ImageOptions::new(AssetUri::new("logo.png")))
    }

    #[test]
    fn test_new_leaves_every_figure_option_unset() {
        // Given / When
        let figure = bare_figure();

        // Then
        assert_eq!(figure.figwidth, None);
        assert!(figure.figclasses.is_empty());
        assert_eq!(figure.caption, None);
        assert!(figure.legend.is_empty());
    }

    #[test]
    fn test_has_no_body_when_nothing_was_written() {
        // Given
        let figure = bare_figure();

        // When / Then
        assert!(!figure.has_body());
    }

    #[test]
    fn test_has_body_with_only_a_caption() {
        // Given
        let mut figure = bare_figure();
        figure.caption = Some(vec![InlineNode::Text("A logo".to_string())]);

        // When / Then
        assert!(figure.has_body());
    }

    #[test]
    fn test_has_body_with_only_a_legend() {
        // Given
        let mut figure = bare_figure();
        figure.legend = vec![Node::Comment];

        // When / Then
        assert!(figure.has_body());
    }

    #[test]
    fn test_serialization_round_trips() {
        // Given
        let mut figure = bare_figure();
        figure.figwidth = Some(FigureWidth::MatchImage);
        figure.figclasses = vec!["framed".to_string()];
        figure.caption = Some(vec![InlineNode::Text("A logo".to_string())]);
        figure.legend = vec![Node::Comment];

        // When
        let json = serde_json::to_string(&figure).expect("should serialize");
        let restored: Figure = serde_json::from_str(&json).expect("should deserialize");

        // Then
        assert_eq!(restored, figure);
    }
}
