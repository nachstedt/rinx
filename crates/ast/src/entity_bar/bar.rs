//! The bar chart's node: what it counts, and how the chart looks.

use rinx_filter::Expr;
use serde::{Deserialize, Serialize};

use crate::chart_color::ChartColor;
use crate::entity_bar::arrangement::{BarArrangement, BarOrientation, BarValueLabels};
use crate::entity_bar::grid::BarGrid;
use crate::entity_bar::source::EntityBarSource;
use crate::image::{ImageAlign, LengthOrPercentage, scaled_width};
use crate::label_rotation::LabelRotation;
use crate::span::Span;
use crate::target_name::TargetName;

/// A bar chart of how many entities each cell of a grid of filters selects —
/// `.. entity-bar::`, and its sphinx-needs spelling `.. needbar::`.
///
/// The fourth presentation of the one question
/// [`EntityTable`](crate::EntityTable), [`EntityFlow`](crate::EntityFlow) and
/// [`EntityPie`](crate::EntityPie) already ask, and a sibling of theirs for
/// their reason: the entities a cell counts are declared in documents this one
/// has never heard of, so only the project index can answer it. What a bar
/// chart adds over a pie is a second dimension — its body is a grid — and an
/// axis to read values off.
///
/// Like a pie, **nothing here is compiled**: the SVG is drawn in the render
/// action, so a library holding a bar chart needs no `diagrams = True`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityBar {
    /// Which of the directive's two names was written.
    pub source: EntityBarSource,
    /// The directive's argument, shown as a heading above the chart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// `:filter:` — narrows the entities *before* any cell counts them, so a
    /// chart can be scoped once rather than in every cell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<Expr>,
    /// The values, already labelled and already in drawing orientation —
    /// `:transpose:` was applied while parsing.
    pub grid: BarGrid,
    /// `:legend:` — draw a key naming each series.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub legend: bool,
    /// `:stacked:` — whether a category's series stand side by side or on
    /// top of each other.
    #[serde(default, skip_serializing_if = "BarArrangement::is_default")]
    pub arrangement: BarArrangement,
    /// `:horizontal:` — which way the bars run.
    #[serde(default, skip_serializing_if = "BarOrientation::is_default")]
    pub orientation: BarOrientation,
    /// `:show_sum:` and `:show_top_sum:` — which values are written onto the
    /// chart.
    #[serde(default, skip_serializing_if = "BarValueLabels::is_default")]
    pub value_labels: BarValueLabels,
    /// `:colors:` — the series colours, in series order.
    ///
    /// Empty uses the built-in palette, and a list shorter than the series is
    /// continued by it — sphinx-needs' `needbar` rule, which differs from its
    /// `needpie`'s (and so from [`EntityPie::colors`](crate::EntityPie)'s),
    /// where a short list repeats.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub colors: Vec<ChartColor>,
    /// `:text_color:` — the colour of every piece of text on the chart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_color: Option<ChartColor>,
    /// `:x_axis_title:` — the caption of the horizontal axis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_axis_title: Option<String>,
    /// `:y_axis_title:` — the caption of the vertical axis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y_axis_title: Option<String>,
    /// `:xlabels_rotation:` — how far the horizontal axis' tick labels turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xlabels_rotation: Option<LabelRotation>,
    /// `:ylabels_rotation:` — how far the vertical axis' tick labels turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ylabels_rotation: Option<LabelRotation>,
    /// `:sum_rotation:` — how far the values `:show_sum:` and `:show_top_sum:`
    /// write turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sum_rotation: Option<LabelRotation>,
    /// `:caption:` — shown under the chart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    /// `:align:` — horizontal placement, the image directives' vocabulary.
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
    /// `:name:` — registers the chart as a `:ref:` target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<TargetName>,
    /// Where the directive was written, so the renderer has a position to
    /// report against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

impl EntityBar {
    /// A chart of `grid` with no options, which the parser then fills in.
    #[must_use]
    pub const fn new(source: EntityBarSource, grid: BarGrid) -> Self {
        Self {
            source,
            title: None,
            filter: None,
            grid,
            legend: false,
            arrangement: BarArrangement::Grouped,
            orientation: BarOrientation::Vertical,
            value_labels: BarValueLabels {
                inside: false,
                at_end: false,
            },
            colors: Vec::new(),
            text_color: None,
            x_axis_title: None,
            y_axis_title: None,
            xlabels_rotation: None,
            ylabels_rotation: None,
            sum_rotation: None,
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
    /// picture here is placed by.
    #[must_use]
    pub fn rendered_width(&self) -> Option<LengthOrPercentage> {
        scaled_width(self.width.as_ref(), self.scale)
    }

    /// Whether a `:scale:` was written that nothing can be applied to, under
    /// [`EntityPie::has_unusable_scale`](crate::EntityPie)'s rule.
    #[must_use]
    pub const fn has_unusable_scale(&self) -> bool {
        self.scale.is_some() && self.width.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart_value::ChartValue;

    fn one_cell_grid() -> BarGrid {
        BarGrid::new(vec![vec![ChartValue::Count(3)]]).unwrap()
    }

    #[test]
    fn test_a_new_chart_carries_its_grid_and_no_options() {
        // Given
        let grid = one_cell_grid();

        // When
        let bar = EntityBar::new(EntityBarSource::EntityBar, grid.clone());

        // Then
        assert_eq!(bar.grid, grid);
        assert_eq!(bar.arrangement, BarArrangement::Grouped);
        assert_eq!(bar.orientation, BarOrientation::Vertical);
        assert_eq!(bar.filter, None);
    }

    #[test]
    fn test_a_scale_applies_to_the_width_it_was_written_beside() {
        // Given
        let mut bar = EntityBar::new(EntityBarSource::NeedBar, one_cell_grid());
        bar.width = Some(LengthOrPercentage::new("400px").unwrap());
        bar.scale = Some(50);

        // When
        let width = bar.rendered_width().unwrap();

        // Then
        assert_eq!(width.to_string(), "200px");
    }

    #[test]
    fn test_a_scale_without_a_width_is_reported_as_unusable() {
        // Given
        let mut bar = EntityBar::new(EntityBarSource::EntityBar, one_cell_grid());
        bar.scale = Some(50);

        // When
        let unusable = bar.has_unusable_scale();

        // Then
        assert!(unusable);
    }

    #[test]
    fn test_a_chart_survives_a_serialization_round_trip() {
        // Given — the node is written to a `.ast` and read back to render
        let mut bar = EntityBar::new(EntityBarSource::NeedBar, one_cell_grid());
        bar.title = Some("Object authors".to_string());
        bar.arrangement = BarArrangement::Stacked;
        bar.value_labels.at_end = true;
        bar.sum_rotation = Some(LabelRotation::parse("45").unwrap());
        bar.colors = vec![ChartColor::parse("navy").unwrap()];
        bar.x_axis_title = Some("Author".to_string());

        // When
        let json = serde_json::to_string(&bar).unwrap();
        let decoded: EntityBar = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(decoded, bar);
    }

    #[test]
    fn test_absent_options_are_left_out_of_the_serialized_form() {
        // Given — a `.ast` is a build artefact stored per document
        let bar = EntityBar::new(EntityBarSource::EntityBar, one_cell_grid());

        // When
        let json = serde_json::to_string(&bar).unwrap();

        // Then
        assert!(!json.contains("arrangement"), "{json}");
        assert!(!json.contains("rotation"), "{json}");
        assert!(!json.contains("span"), "{json}");
    }
}
