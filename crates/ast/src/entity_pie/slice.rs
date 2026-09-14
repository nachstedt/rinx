//! One wedge of a pie chart: its label, and where its size comes from.

use rusty_sphinx_filter::Expr;
use serde::{Deserialize, Serialize};

/// What decides how large a wedge is.
///
/// Two cases because sphinx-needs' `needpie` admits two kinds of content line,
/// and refusing the second would break documents that mix them: a line is
/// either a filter over the entity graph, or a number written outright.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SliceSource {
    /// A filter whose *count* of matching entities is the wedge's size.
    ///
    /// `None` selects every entity, which is the encoding
    /// [`EntityTable::filter`](crate::EntityTable) and
    /// [`EntityFlow::filter`](crate::EntityFlow) already use and the rule the
    /// parser's shared filter reader already documents: a filter that could
    /// not be parsed leaves the directive selecting everything, because the
    /// diagnostic beside it says what is wrong and an empty result on top of
    /// that would hide what the author was reaching for. The slice is kept
    /// either way, so the labels still line up with the lines they were
    /// written against.
    Filter(Option<Expr>),
    /// A number written in the body instead of a filter, for a chart whose
    /// data does not come from the graph at all.
    Count(u64),
}

/// One wedge: what it is called, and what makes it that size.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PieSlice {
    /// The wedge's label, taken from the `:labels:` option by position.
    ///
    /// `None` when the author wrote fewer labels than content lines. The wedge
    /// is still drawn — a missing name is not a reason to lose the data — and
    /// the mismatch is reported while parsing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Where the wedge's size comes from.
    pub source: SliceSource,
}

impl PieSlice {
    /// A wedge counting the entities a filter selects.
    #[must_use]
    pub fn from_filter(filter: Option<Expr>) -> Self {
        Self {
            label: None,
            source: SliceSource::Filter(filter),
        }
    }

    /// A wedge of a size written outright.
    #[must_use]
    pub const fn from_count(count: u64) -> Self {
        Self {
            label: None,
            source: SliceSource::Count(count),
        }
    }

    /// The label to draw, falling back to the wedge's position.
    ///
    /// A wedge with no name still needs one on the chart, and its ordinal is
    /// the only thing that distinguishes it — the same information the author
    /// would count lines to find.
    #[must_use]
    pub fn display_label(&self, position: usize) -> String {
        self.label
            .clone()
            .unwrap_or_else(|| format!("#{}", position + 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_filter_slice_carries_the_expression_it_counts() {
        // Given
        let expr = rusty_sphinx_filter::parse_filter(r#"type == "req""#).unwrap();

        // When
        let slice = PieSlice::from_filter(Some(expr.clone()));

        // Then
        assert_eq!(slice.source, SliceSource::Filter(Some(expr)));
        assert_eq!(slice.label, None);
    }

    #[test]
    fn test_an_unparseable_filter_leaves_the_slice_in_place_selecting_everything() {
        // Given — the rule every other filtered directive follows: the
        // diagnostic already says what broke, and losing the wedge would also
        // lose the alignment between labels and the lines they were written
        // against
        let slice = PieSlice::from_filter(None);

        // When
        let source = slice.source;

        // Then
        assert_eq!(source, SliceSource::Filter(None));
    }

    #[test]
    fn test_a_count_slice_carries_the_number_written() {
        // Given
        let written = 12;

        // When
        let slice = PieSlice::from_count(written);

        // Then
        assert_eq!(slice.source, SliceSource::Count(12));
    }

    #[test]
    fn test_an_unlabelled_slice_falls_back_to_its_position() {
        // Given
        let slice = PieSlice::from_count(3);

        // When
        let label = slice.display_label(2);

        // Then
        assert_eq!(label, "#3");
    }

    #[test]
    fn test_a_labelled_slice_keeps_its_own_name() {
        // Given
        let mut slice = PieSlice::from_count(3);
        slice.label = Some("Hazards".to_string());

        // When
        let label = slice.display_label(0);

        // Then
        assert_eq!(label, "Hazards");
    }

    #[test]
    fn test_a_slice_survives_a_serialization_round_trip() {
        // Given — the node is written to a `.ast` and read back to render
        let mut slice =
            PieSlice::from_filter(Some(rusty_sphinx_filter::parse_filter("asil").unwrap()));
        slice.label = Some("ASIL D".to_string());

        // When
        let json = serde_json::to_string(&slice).unwrap();
        let decoded: PieSlice = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(decoded, slice);
    }

    #[test]
    fn test_an_absent_label_is_left_out_of_the_serialized_form() {
        // Given — a `.ast` is a build artefact stored per document
        let slice = PieSlice::from_count(1);

        // When
        let json = serde_json::to_string(&slice).unwrap();

        // Then
        assert!(!json.contains("label"), "{json}");
    }
}
