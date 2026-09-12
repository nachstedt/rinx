use std::fmt;

use serde::{Deserialize, Serialize};

/// How a flowchart is laid out.
///
/// Two values, not four, because `PlantUML` has two: `left to right direction`
/// and the top-to-bottom default. sphinx-needs' own `:root_direction:` is about
/// which way the *graph* is walked rather than which way it is drawn, so there
/// is no third spelling here that would quietly do nothing — an unrecognized
/// value is refused by [`FlowDirection::new`] and reported against the option
/// line, rather than becoming a `PlantUML` statement nobody reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FlowDirection {
    /// `PlantUML`'s own default, so it emits no statement at all.
    #[default]
    TopToBottom,
    /// `left to right direction`.
    LeftToRight,
}

/// The accepted spellings, first of each pair being the canonical one.
const SPELLINGS: [(&str, FlowDirection); 4] = [
    ("TB", FlowDirection::TopToBottom),
    ("top-to-bottom", FlowDirection::TopToBottom),
    ("LR", FlowDirection::LeftToRight),
    ("left-to-right", FlowDirection::LeftToRight),
];

/// A `:direction:` value naming no layout this build can draw.
///
/// Carries what was written so the diagnostic can quote it, and
/// [`InvalidFlowDirection::accepted`] lists what would have worked — the value
/// set is small enough that naming it is more useful than describing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidFlowDirection {
    written: String,
}

impl InvalidFlowDirection {
    /// The spellings that would have been accepted, comma-separated.
    #[must_use]
    pub fn accepted() -> String {
        SPELLINGS
            .iter()
            .map(|(spelling, _)| *spelling)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl fmt::Display for InvalidFlowDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "expects one of {}, found '{}'",
            Self::accepted(),
            self.written
        )
    }
}

impl FlowDirection {
    /// The direction `value` names, or why it names none.
    ///
    /// Case-insensitive, because `TB`/`LR` are conventionally shouted and
    /// `left-to-right` conventionally is not.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidFlowDirection`] when `value` is not one of the four
    /// accepted spellings.
    pub fn new(value: &str) -> Result<Self, InvalidFlowDirection> {
        let wanted = value.trim();
        SPELLINGS
            .iter()
            .find(|(spelling, _)| spelling.eq_ignore_ascii_case(wanted))
            .map(|(_, direction)| *direction)
            .ok_or_else(|| InvalidFlowDirection {
                written: wanted.to_string(),
            })
    }

    /// Whether this is the direction an omitted `:direction:` means.
    ///
    /// Exists so the default stays out of a serialized node — an option nobody
    /// wrote should cost nothing in a `.ast` file, which is a build artefact
    /// stored per document.
    #[must_use]
    pub const fn is_top_to_bottom(&self) -> bool {
        matches!(self, Self::TopToBottom)
    }

    /// The `PlantUML` statement that selects this layout, if one is needed.
    ///
    /// `None` for the default: emitting `top to bottom direction` would change
    /// the bytes of every flowchart that never asked for a direction, and the
    /// bytes are the compiled picture's filename.
    #[must_use]
    pub const fn statement(self) -> Option<&'static str> {
        match self {
            Self::TopToBottom => None,
            Self::LeftToRight => Some("left to right direction"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_every_accepted_spelling_names_a_direction() {
        // Given
        let written = ["TB", "top-to-bottom", "LR", "left-to-right"];

        // When
        let parsed: Vec<FlowDirection> = written
            .iter()
            .map(|value| FlowDirection::new(value).unwrap())
            .collect();

        // Then
        assert_eq!(
            parsed,
            [
                FlowDirection::TopToBottom,
                FlowDirection::TopToBottom,
                FlowDirection::LeftToRight,
                FlowDirection::LeftToRight,
            ]
        );
    }

    #[test]
    fn test_a_spelling_is_read_whatever_its_case() {
        // Given
        let written = "lr";

        // When
        let parsed = FlowDirection::new(written).unwrap();

        // Then
        assert_eq!(parsed, FlowDirection::LeftToRight);
    }

    #[test]
    fn test_surrounding_whitespace_is_not_part_of_the_value() {
        // Given — an option line's value arrives trimmed, but a nested
        // directive body may not have been
        let written = "  TB  ";

        // When
        let parsed = FlowDirection::new(written).unwrap();

        // Then
        assert_eq!(parsed, FlowDirection::TopToBottom);
    }

    #[test]
    fn test_a_direction_plantuml_cannot_draw_is_refused_with_what_was_written() {
        // Given — graphviz' `RL` has no PlantUML equivalent
        let written = "RL";

        // When
        let problem = FlowDirection::new(written).unwrap_err();

        // Then
        assert!(problem.to_string().contains("RL"), "{problem}");
        assert!(problem.to_string().contains("left-to-right"), "{problem}");
    }

    #[test]
    fn test_only_the_non_default_direction_emits_a_statement() {
        // Given
        let directions = [FlowDirection::TopToBottom, FlowDirection::LeftToRight];

        // When
        let statements: Vec<Option<&str>> = directions
            .iter()
            .map(|direction| direction.statement())
            .collect();

        // Then
        assert_eq!(statements, [None, Some("left to right direction")]);
    }

    #[test]
    fn test_the_default_is_plantuml_s_own() {
        // Given / When
        let direction = FlowDirection::default();

        // Then
        assert_eq!(direction, FlowDirection::TopToBottom);
    }

    #[test]
    fn test_only_the_default_direction_is_left_out_of_a_serialized_node() {
        // Given
        let directions = [FlowDirection::TopToBottom, FlowDirection::LeftToRight];

        // When
        let skipped: Vec<bool> = directions
            .iter()
            .map(FlowDirection::is_top_to_bottom)
            .collect();

        // Then
        assert_eq!(skipped, [true, false]);
    }
}
