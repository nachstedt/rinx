//! One value a chart over the entity graph draws: a count to take, or a number
//! written outright.
//!
//! Flat at the crate root, beside [`crate::chart_color`], because it is shared
//! by two node families: a pie's wedge ([`crate::PieSlice`]) and a bar chart's
//! cell ([`crate::BarGrid`]). sphinx-needs' `needpie` and `needbar` admit the
//! same two kinds of content, so the two charts differ in presentation only —
//! which is the whole shape of `docs/decisions/017-entity-pie.md`.

use rusty_sphinx_filter::Expr;
use serde::{Deserialize, Serialize};

/// What decides one value of a chart.
///
/// Two cases because sphinx-needs admits two kinds of content, and refusing
/// the second would break documents that mix them: a value is either a filter
/// over the entity graph, or a number written outright.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChartValue {
    /// A filter whose *count* of matching entities is the value.
    ///
    /// `None` selects every entity, which is the encoding
    /// [`EntityTable::filter`](crate::EntityTable) and
    /// [`EntityFlow::filter`](crate::EntityFlow) already use and the rule the
    /// parser's shared filter reader already documents: a filter that could
    /// not be parsed leaves the value selecting everything, because the
    /// diagnostic beside it says what is wrong and an empty result on top of
    /// that would hide what the author was reaching for. The value is kept
    /// either way, so labels still line up with what they were written
    /// against.
    Filter(Option<Expr>),
    /// A number written in the body instead of a filter, for a chart whose
    /// data does not come from the graph at all.
    Count(u64),
}

impl ChartValue {
    /// Whether the value has to be counted from the project index at all.
    ///
    /// A chart whose every value is a written number never touches the index,
    /// which is the one case a chart costs nothing to draw.
    #[must_use]
    pub const fn is_counted(&self) -> bool {
        matches!(self, Self::Filter(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_filter_value_is_counted() {
        // Given
        let value = ChartValue::Filter(None);

        // When
        let counted = value.is_counted();

        // Then
        assert!(counted);
    }

    #[test]
    fn test_a_written_number_is_not_counted() {
        // Given
        let value = ChartValue::Count(7);

        // When
        let counted = value.is_counted();

        // Then
        assert!(!counted);
    }

    #[test]
    fn test_a_value_survives_a_serialization_round_trip() {
        // Given — the node is written to a `.ast` and read back to render
        let values = [
            ChartValue::Filter(Some(
                rusty_sphinx_filter::parse_filter(r#"type == "req""#).unwrap(),
            )),
            ChartValue::Filter(None),
            ChartValue::Count(12),
        ];

        // When
        let json = serde_json::to_string(&values).unwrap();
        let decoded: Vec<ChartValue> = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(decoded, values);
    }
}
