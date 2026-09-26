//! The flowchart's node: what it selects, what it connects, how it looks.

use rinx_filter::Expr;
use serde::{Deserialize, Serialize};

use crate::entity_flow::direction::FlowDirection;
use crate::entity_flow::source::EntityFlowSource;
use crate::image::{ImageAlign, LengthOrPercentage, scaled_width};
use crate::span::Span;
use crate::target_name::TargetName;

/// A picture of the entities matching a filter and the relations between them
/// — `.. entity-flow::`, and its sphinx-needs spelling `.. needflow::`.
///
/// The node carries a *question*, exactly as [`EntityTable`](crate::EntityTable)
/// does and for the same reason: the entities a flowchart draws are declared in
/// documents this one has never heard of, so only the project index can answer
/// it. What separates the two is only the presentation — a table renders its
/// answer as rows, a flowchart as generated `PlantUML` that a build action
/// compiles.
///
/// It is therefore *not* a [`Uml`](crate::Uml) with a template: there is no
/// template, and nothing an author wrote reaches `PlantUML` at all. What the
/// two do share is everything after the text exists — the hash, the `.puml`
/// file, the compile action and the `<img>` — because a second population of
/// diagram hashes is a second thing that can drift out of step with what the
/// build actually compiled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityFlow {
    /// Which of the directive's two names was written.
    pub source: EntityFlowSource,
    /// `:filter:` — which entities to draw. `None` draws every entity in the
    /// project, which is what an omitted filter means in sphinx-needs too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<Expr>,
    /// `:relations:`, or sphinx-needs' `:link_types:` spelling of it — which
    /// relations become edges, in the order written.
    ///
    /// `None` draws every relation the schema declares, which is *not*
    /// sphinx-needs' default of `links`: a schema here names its own
    /// relations, and `links` may well not be one of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relations: Option<Vec<String>>,
    /// `:show-link-names:` — label each edge with its relation.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub show_link_names: bool,
    /// `:direction:` — how the graph is laid out.
    #[serde(default, skip_serializing_if = "FlowDirection::is_top_to_bottom")]
    pub direction: FlowDirection,
    /// `:config:` — the name of a `PlantUML` preamble to prepend. A *name*
    /// resolved against the site config, never a path (ADR-001), read through
    /// the very same table a written diagram's `:config:` is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
    /// `:debug:` — also show the generated `PlantUML` source, as a code block
    /// after the picture. The option earns its place here more than it does on
    /// a written diagram: this text is generated, so it is the only way to see
    /// what was actually drawn.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub debug: bool,
    /// `:caption:` — shown under the picture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    /// `:align:` — horizontal placement, reusing the image directives' own
    /// vocabulary so a diagram and a picture align by the same rules.
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
    /// `:name:` — reuses [`TargetName`] like every other directive that takes
    /// one, so it registers in `ProjectIndex::targets` with no conversion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<TargetName>,
    /// Where the directive was written, so the renderer — which is where the
    /// picture is actually generated — has a position to report against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

impl EntityFlow {
    /// A flowchart drawing the whole graph, with no options.
    #[must_use]
    pub const fn new(source: EntityFlowSource) -> Self {
        Self {
            source,
            filter: None,
            relations: None,
            show_link_names: false,
            direction: FlowDirection::TopToBottom,
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

    /// The width to render with, `:scale:` applied.
    ///
    /// The same arithmetic an image and a written diagram are placed by, from
    /// the one function all three call.
    #[must_use]
    pub fn rendered_width(&self) -> Option<LengthOrPercentage> {
        scaled_width(self.width.as_ref(), self.scale)
    }

    /// Whether a `:scale:` was written that nothing can be applied to.
    ///
    /// The renderer never opens the compiled SVG, so there is no natural size
    /// to scale — [`Uml::has_unusable_scale`](crate::Uml::has_unusable_scale)'s
    /// rule, reported the same way, by the parser.
    #[must_use]
    pub const fn has_unusable_scale(&self) -> bool {
        self.scale.is_some() && self.width.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_new_flowchart_draws_everything_and_carries_no_options() {
        // Given
        let source = EntityFlowSource::EntityFlow;

        // When
        let flow = EntityFlow::new(source);

        // Then
        assert_eq!(flow.filter, None);
        assert_eq!(flow.relations, None);
        assert!(!flow.show_link_names);
        assert_eq!(flow.direction, FlowDirection::TopToBottom);
        assert!(flow.classes.is_empty());
    }

    #[test]
    fn test_a_scale_applies_to_the_width_it_was_written_beside() {
        // Given
        let mut flow = EntityFlow::new(EntityFlowSource::NeedFlow);
        flow.width = Some(LengthOrPercentage::new("400px").unwrap());
        flow.scale = Some(50);

        // When
        let width = flow.rendered_width().unwrap();

        // Then
        assert_eq!(width.to_string(), "200px");
    }

    #[test]
    fn test_a_scale_without_a_width_is_reported_as_unusable() {
        // Given
        let mut flow = EntityFlow::new(EntityFlowSource::EntityFlow);
        flow.scale = Some(50);

        // When
        let unusable = flow.has_unusable_scale();

        // Then
        assert!(unusable);
        assert_eq!(flow.rendered_width(), None);
    }

    #[test]
    fn test_a_flowchart_survives_a_serialization_round_trip() {
        // Given — the node is written to a `.ast` and read back to render
        let mut flow = EntityFlow::new(EntityFlowSource::NeedFlow);
        flow.filter = Some(rinx_filter::parse_filter(r#"type == "req""#).unwrap());
        flow.relations = Some(vec!["links".to_string()]);
        flow.show_link_names = true;
        flow.direction = FlowDirection::LeftToRight;
        flow.classes = vec!["wide".to_string()];

        // When
        let json = serde_json::to_string(&flow).unwrap();
        let decoded: EntityFlow = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(decoded, flow);
    }

    #[test]
    fn test_absent_options_are_left_out_of_the_serialized_form() {
        // Given — a `.ast` is a build artefact stored per document, so an
        // unset option should cost nothing
        let flow = EntityFlow::new(EntityFlowSource::EntityFlow);

        // When
        let json = serde_json::to_string(&flow).unwrap();

        // Then
        assert!(!json.contains("filter"), "{json}");
        assert!(!json.contains("direction"), "{json}");
        assert!(!json.contains("span"), "{json}");
    }
}
