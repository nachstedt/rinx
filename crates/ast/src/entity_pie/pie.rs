//! The pie chart's node: what it counts, and how the chart looks.

use rinx_filter::Expr;
use serde::{Deserialize, Serialize};

use crate::chart_color::ChartColor;
use crate::entity_pie::slice::PieSlice;
use crate::entity_pie::source::EntityPieSource;
use crate::image::{ImageAlign, LengthOrPercentage, scaled_width};
use crate::span::Span;
use crate::target_name::TargetName;

/// A pie chart of how many entities each of several filters selects —
/// `.. entity-pie::`, and its sphinx-needs spelling `.. needpie::`.
///
/// The node carries a *question*, exactly as [`EntityTable`](crate::EntityTable)
/// and [`EntityFlow`](crate::EntityFlow) do and for the same reason: the
/// entities a wedge counts are declared in documents this one has never heard
/// of, so only the project index can answer it. What separates the three is
/// only the presentation — rows, a graph, or proportions.
///
/// What separates it from a flowchart in particular is that **nothing here is
/// compiled**. A pie is drawn as SVG in the render action itself, so it needs
/// no `.puml` file, no `PlantUML`, and no `diagrams = True` on its library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityPie {
    /// Which of the directive's two names was written.
    pub source: EntityPieSource,
    /// The directive's argument, shown as a heading above the chart.
    ///
    /// Unlike [`EntityTable`](crate::EntityTable), which rejects an argument,
    /// and [`EntityFlow`](crate::EntityFlow), which rejects content, a pie
    /// takes both — that is sphinx-needs' shape, and every `.. needpie::` in
    /// the benchmark corpus writes a title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// `:filter:` — narrows the entities *before* any wedge counts them, so a
    /// chart can be scoped once rather than in every content line.
    ///
    /// `None` considers every entity in the project, which is what an omitted
    /// filter means in sphinx-needs too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<Expr>,
    /// The wedges, in the order their content lines were written.
    ///
    /// Order is the whole interface between the body and `:labels:`, which
    /// pairs with it by position.
    pub slices: Vec<PieSlice>,
    /// `:legend:` — draw a key naming each wedge beside the chart.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub legend: bool,
    /// `:colors:` — the wedge colours, in the order the wedges are drawn.
    ///
    /// Empty uses the built-in palette, and a list shorter than `slices`
    /// repeats — which is what the palette does anyway, so the two cases need
    /// no separate handling. Already validated: a [`ChartColor`] can only hold
    /// a colour this build can draw, so the renderer never asks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub colors: Vec<ChartColor>,
    /// `:text_color:` — the colour of the percentages drawn on the wedges.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_color: Option<ChartColor>,
    /// `:caption:` — shown under the chart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    /// `:align:` — horizontal placement, reusing the image directives' own
    /// vocabulary so a chart and a picture align by the same rules.
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
    /// chart is actually drawn — has a position to report against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

impl EntityPie {
    /// An empty chart with no options, which the parser then fills in.
    #[must_use]
    pub const fn new(source: EntityPieSource) -> Self {
        Self {
            source,
            title: None,
            filter: None,
            slices: Vec::new(),
            legend: false,
            colors: Vec::new(),
            text_color: None,
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
    /// The same arithmetic an image, a written diagram and a flowchart are
    /// placed by, from the one function all of them call.
    #[must_use]
    pub fn rendered_width(&self) -> Option<LengthOrPercentage> {
        scaled_width(self.width.as_ref(), self.scale)
    }

    /// Whether a `:scale:` was written that nothing can be applied to.
    ///
    /// [`EntityFlow::has_unusable_scale`](crate::EntityFlow)'s rule, reported
    /// the same way, by the parser — but for a different reason worth knowing:
    /// a flowchart's SVG is never opened while rendering, whereas a pie's is
    /// *generated* here at a size this build chooses, so there is still no
    /// natural size for a bare percentage to scale.
    #[must_use]
    pub const fn has_unusable_scale(&self) -> bool {
        self.scale.is_some() && self.width.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_new_chart_counts_nothing_and_carries_no_options() {
        // Given
        let source = EntityPieSource::EntityPie;

        // When
        let pie = EntityPie::new(source);

        // Then
        assert_eq!(pie.filter, None);
        assert!(pie.slices.is_empty());
        assert!(!pie.legend);
        assert!(pie.colors.is_empty());
        assert_eq!(pie.title, None);
    }

    #[test]
    fn test_a_scale_applies_to_the_width_it_was_written_beside() {
        // Given
        let mut pie = EntityPie::new(EntityPieSource::NeedPie);
        pie.width = Some(LengthOrPercentage::new("400px").unwrap());
        pie.scale = Some(50);

        // When
        let width = pie.rendered_width().unwrap();

        // Then
        assert_eq!(width.to_string(), "200px");
    }

    #[test]
    fn test_a_scale_without_a_width_is_reported_as_unusable() {
        // Given
        let mut pie = EntityPie::new(EntityPieSource::EntityPie);
        pie.scale = Some(50);

        // When
        let unusable = pie.has_unusable_scale();

        // Then
        assert!(unusable);
        assert_eq!(pie.rendered_width(), None);
    }

    #[test]
    fn test_a_chart_survives_a_serialization_round_trip() {
        // Given — the node is written to a `.ast` and read back to render
        let mut pie = EntityPie::new(EntityPieSource::NeedPie);
        pie.title = Some("Safety Artifacts by Type".to_string());
        pie.filter = Some(rinx_filter::parse_filter(r#"type == "req""#).unwrap());
        pie.slices = vec![
            PieSlice::from_filter(Some(
                rinx_filter::parse_filter(r#"id.startswith("SG_")"#).unwrap(),
            )),
            PieSlice::from_count(7),
        ];
        pie.legend = true;
        pie.colors = vec![ChartColor::parse("#4c72b0").unwrap()];
        pie.classes = vec!["wide".to_string()];

        // When
        let json = serde_json::to_string(&pie).unwrap();
        let decoded: EntityPie = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(decoded, pie);
    }

    #[test]
    fn test_absent_options_are_left_out_of_the_serialized_form() {
        // Given — a `.ast` is a build artefact stored per document, so an
        // unset option should cost nothing
        let pie = EntityPie::new(EntityPieSource::EntityPie);

        // When
        let json = serde_json::to_string(&pie).unwrap();

        // Then
        assert!(!json.contains("filter"), "{json}");
        assert!(!json.contains("legend"), "{json}");
        assert!(!json.contains("span"), "{json}");
    }
}
