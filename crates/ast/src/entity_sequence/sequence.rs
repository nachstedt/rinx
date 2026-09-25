//! The sequence diagram's node: where the walk starts, what it follows, how it
//! looks.

use std::num::NonZeroU32;

use rinx_filter::Expr;
use serde::{Deserialize, Serialize};

use crate::entity::EntityId;
use crate::entity_sequence::source::EntitySequenceSource;
use crate::image::{ImageAlign, LengthOrPercentage, scaled_width};
use crate::non_empty_vector::NonEmptyVector;
use crate::span::Span;
use crate::target_name::TargetName;

/// A sequence diagram walked through the entity graph —
/// `.. entity-sequence::`, and its sphinx-needs spelling `.. needsequence::`.
///
/// The walk begins at each `start` entity, the first *sender*. Following one
/// of `relations` from a sender reaches a *message* entity, and following the
/// same relations on from the message reaches its *receivers*; each receiver
/// is drawn as a lifeline and becomes a sender in turn. A message is drawn as
/// an arrow labelled with its title.
///
/// Like [`EntityFlow`](crate::EntityFlow), the node carries a *question* the
/// project index answers while rendering, and shares everything after the
/// generated text exists with the written diagrams.
///
/// `start` and `relations` are both non-empty by construction: a diagram with
/// nowhere to begin, or with nothing saying which edges are messages, degrades
/// to an error block while parsing instead of reaching the renderer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntitySequence {
    /// Which of the directive's two names was written.
    pub source: EntitySequenceSource,
    /// `:start:` — the entities the walk begins at, in the order written.
    pub start: NonEmptyVector<EntityId>,
    /// `:relations:`, or sphinx-needs' `:link_types:` spelling of it — the
    /// relations that lead from a sender to a message and on to its receivers.
    ///
    /// Mandatory, unlike sphinx-needs' default of `links`: a schema here names
    /// its own relations, and "every relation it declares" would walk edges
    /// that are no message at all.
    pub relations: NonEmptyVector<String>,
    /// `:filter:` — which *receivers* to keep. A receiver the filter rejects
    /// gets no arrow and is not walked on from, exactly as in sphinx-needs; the
    /// start entities are never filtered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<Expr>,
    /// `:max-items:` — the most messages to draw. `None` draws every one,
    /// which is also what sphinx-needs' `:max_items: 0` means.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_items: Option<NonZeroU32>,
    /// `:config:` — the name of a `PlantUML` preamble to prepend, resolved
    /// through the same table every diagram's `:config:` is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
    /// `:debug:` — also show the generated `PlantUML` source.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub debug: bool,
    /// The directive's argument, or `:caption:` — shown under the picture.
    /// sphinx-needs takes the argument as the caption, so both are accepted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    /// `:align:` — horizontal placement, in the image directives' vocabulary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<ImageAlign>,
    /// `:scale:` — a percentage of `:width:`, stored as written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<u32>,
    /// `:width:` — a length or a percentage of the available width.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<LengthOrPercentage>,
    /// `:class:` — space-separated class names, already split.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub classes: Vec<String>,
    /// `:name:` — registers the diagram as an ordinary `:ref:` target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<TargetName>,
    /// Where the directive was written, so the renderer has a position to
    /// report an unknown start or an empty walk against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

impl EntitySequence {
    /// A sequence diagram walking `relations` from `start`, with no options.
    #[must_use]
    pub const fn new(
        source: EntitySequenceSource,
        start: NonEmptyVector<EntityId>,
        relations: NonEmptyVector<String>,
    ) -> Self {
        Self {
            source,
            start,
            relations,
            filter: None,
            max_items: None,
            config: None,
            debug: false,
            caption: None,
            align: None,
            scale: None,
            width: None,
            classes: Vec::new(),
            name: None,
            span: None,
        }
    }

    /// The width to render with, `:scale:` applied — the arithmetic every
    /// image and diagram is placed by.
    #[must_use]
    pub fn rendered_width(&self) -> Option<LengthOrPercentage> {
        scaled_width(self.width.as_ref(), self.scale)
    }

    /// Whether a `:scale:` was written that nothing can be applied to, for
    /// [`Uml::has_unusable_scale`](crate::Uml::has_unusable_scale)'s reason.
    #[must_use]
    pub const fn has_unusable_scale(&self) -> bool {
        self.scale.is_some() && self.width.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sequence() -> EntitySequence {
        EntitySequence::new(
            EntitySequenceSource::EntitySequence,
            NonEmptyVector::single(EntityId::new("COMP_UI").unwrap()),
            NonEmptyVector::single("sends".to_string()),
        )
    }

    #[test]
    fn test_a_new_sequence_carries_its_walk_and_no_options() {
        // Given / When
        let sequence = sequence();

        // Then
        assert_eq!(sequence.start.first().as_str(), "COMP_UI");
        assert_eq!(sequence.relations.as_slice(), ["sends"]);
        assert_eq!(sequence.filter, None);
        assert_eq!(sequence.max_items, None);
        assert!(sequence.classes.is_empty());
    }

    #[test]
    fn test_a_scale_applies_to_the_width_it_was_written_beside() {
        // Given
        let mut sequence = sequence();
        sequence.width = Some(LengthOrPercentage::new("400px").unwrap());
        sequence.scale = Some(50);

        // When
        let width = sequence.rendered_width().unwrap();

        // Then
        assert_eq!(width.to_string(), "200px");
        assert!(!sequence.has_unusable_scale());
    }

    #[test]
    fn test_a_scale_without_a_width_is_reported_as_unusable() {
        // Given
        let mut sequence = sequence();
        sequence.scale = Some(50);

        // When
        let unusable = sequence.has_unusable_scale();

        // Then
        assert!(unusable);
    }

    #[test]
    fn test_a_sequence_survives_a_serialization_round_trip() {
        // Given — the node is written to a `.ast` and read back to render
        let mut sequence = sequence();
        sequence.filter = Some(rinx_filter::parse_filter(r#"type == "comp""#).unwrap());
        sequence.max_items = NonZeroU32::new(3);
        sequence.caption = Some("Startup".to_string());

        // When
        let json = serde_json::to_string(&sequence).unwrap();
        let decoded: EntitySequence = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(decoded, sequence);
    }

    #[test]
    fn test_an_empty_start_list_is_refused_on_load() {
        // Given — a stale or hand-edited `.ast` with nowhere to begin the walk
        let json = r#"{"source":"EntitySequence","start":[],"relations":["sends"]}"#;

        // When
        let decoded = serde_json::from_str::<EntitySequence>(json);

        // Then
        assert!(decoded.is_err());
    }

    #[test]
    fn test_absent_options_are_left_out_of_the_serialized_form() {
        // Given
        let sequence = sequence();

        // When
        let json = serde_json::to_string(&sequence).unwrap();

        // Then
        assert_eq!(
            json,
            r#"{"source":"EntitySequence","start":["COMP_UI"],"relations":["sends"]}"#
        );
    }
}
