use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;

/// How deep `:numbered:` numbers the subtree it starts.
///
/// The option has two spellings that mean genuinely different things — the
/// bare flag numbers everything it reaches, `:numbered: 2` numbers only the
/// first two levels — so this is an enum rather than an `Option<usize>` inside
/// an `Option`. A depth of zero is not a third case but a *disabled* one, so
/// it is unrepresentable here: the parser maps `:numbered: 0` to no numbering
/// at all, exactly as Sphinx does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumberedDepth {
    /// `:numbered:` — number every level the toctree reaches.
    Unlimited,
    /// `:numbered: N` — number the first `N` levels; deeper sections are left
    /// unnumbered rather than given a truncated number.
    Levels(NonZeroUsize),
}

impl NumberedDepth {
    /// Whether a section at `depth` (1-based, the toctree's own entries being
    /// depth 1) still receives a number.
    #[must_use]
    pub const fn covers(self, depth: usize) -> bool {
        match self {
            Self::Unlimited => true,
            Self::Levels(levels) => depth <= levels.get(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn levels(n: usize) -> NumberedDepth {
        NumberedDepth::Levels(NonZeroUsize::new(n).expect("non-zero"))
    }

    #[test]
    fn test_unlimited_covers_every_depth() {
        // Given
        let depth = NumberedDepth::Unlimited;

        // When / Then
        assert!(depth.covers(1));
        assert!(depth.covers(99));
    }

    #[test]
    fn test_levels_covers_up_to_and_including_its_limit() {
        // Given — `:numbered: 2`.
        let depth = levels(2);

        // When / Then
        assert!(depth.covers(1));
        assert!(depth.covers(2));
    }

    #[test]
    fn test_levels_stops_beyond_its_limit() {
        // Given — `:numbered: 2`; a third level is left unnumbered rather than
        // given a truncated number.
        let depth = levels(2);

        // When / Then
        assert!(!depth.covers(3));
    }

    #[test]
    fn test_numbered_depth_round_trips_through_json() {
        // Given
        let depth = levels(3);

        // When
        let json = serde_json::to_string(&depth).expect("serializes");
        let restored: NumberedDepth = serde_json::from_str(&json).expect("deserializes");

        // Then
        assert_eq!(restored, depth);
    }
}
