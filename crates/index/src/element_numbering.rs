//! What the index records for `numfig` numbering and the `:numref:` role.
//!
//! Split the way `toctrees` and `section_numbers` are split, and for the same
//! reason. Each document's [`NumberingStep`]s and the labels it defines
//! ([`NumrefTarget`]) depend only on that document, so they merge — which is
//! what lets the live preview renumber with a fresh document in a stale index.
//! The numbers themselves ([`ElementNumbers`]) depend on the whole toctree
//! graph, so they are recomputed rather than merged.

use rinx_ast::{EnumerableKind, SectionId};
use serde::{Deserialize, Serialize};

/// How many components of the section number every element's number starts
/// with — Sphinx's `numfig_secnum_depth`, whose default is 1. Here rather than
/// with the walk that reads it because the site config's default must be the
/// same number, and the renderer that owns that config cannot see the analyzer.
pub const DEFAULT_NUMFIG_SECNUM_DEPTH: usize = 1;

/// One thing Sphinx's numbering walk meets in a document, in document order.
///
/// Only the two things that matter to it are recorded: an element to number,
/// and a toctree to descend through — the documents a toctree lists are
/// numbered *at the point it is written*, between the elements before and
/// after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumberingStep {
    /// A captioned figure, table or code block. Its position among the
    /// document's elements (not among all steps) is what
    /// [`rinx_ast::enumerable_elements`] identifies it by.
    Element {
        kind: EnumerableKind,
        /// The sections it is written inside, outermost first.
        sections: Vec<SectionId>,
    },
    /// The document's toctree at this position among its top-level
    /// toctrees — an index into `ProjectIndex::toctrees`.
    Toctree {
        position: usize,
        /// The sections it is written inside, outermost first.
        sections: Vec<SectionId>,
    },
}

/// What a label a `:numref:` may name points at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NumrefTarget {
    /// The document it is in.
    pub doc_path: String,
    pub subject: NumrefSubject,
}

/// The numbered thing a label names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumrefSubject {
    /// A captioned element, by its position among the document's elements.
    Element {
        ordinal: usize,
        kind: EnumerableKind,
    },
    /// A top-level heading, by its section id. A label on the document's
    /// title names one too; the title has no section number of its own, so
    /// it shows the document's (see `DocumentNumbers::number_at`).
    Section(SectionId),
}

impl NumrefSubject {
    /// Which kind of thing this is, for picking its `numfig_format`.
    #[must_use]
    pub const fn kind(&self) -> EnumerableKind {
        match self {
            Self::Element { kind, .. } => *kind,
            Self::Section(_) => EnumerableKind::Section,
        }
    }
}

/// The numbers `numfig` gave one document's elements, by position.
///
/// A position with no number is an element no toctree walk reached — its
/// document is an orphan — or a document numbered before `numfig` was on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementNumbers(Vec<Option<Vec<usize>>>);

impl ElementNumbers {
    /// Records the number of the element at `ordinal`.
    pub fn set(&mut self, ordinal: usize, number: Vec<usize>) {
        if self.0.len() <= ordinal {
            self.0.resize(ordinal + 1, None);
        }
        self.0[ordinal] = Some(number);
    }

    /// The number of the element at `ordinal`, if it was given one.
    #[must_use]
    pub fn get(&self, ordinal: usize) -> Option<&[usize]> {
        self.0.get(ordinal).and_then(Option::as_deref)
    }
}

/// Joins number components the way Sphinx shows them: `1.2.3`.
#[must_use]
pub fn join_number(number: &[usize]) -> String {
    number
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_element_numbers_leave_a_skipped_position_unnumbered() {
        // Given
        let mut numbers = ElementNumbers::default();

        // When
        numbers.set(2, vec![1, 3]);

        // Then
        assert_eq!(numbers.get(0), None);
        assert_eq!(numbers.get(2), Some([1, 3].as_slice()));
        assert_eq!(numbers.get(9), None);
    }

    #[test]
    fn test_element_numbers_round_trip_through_json() {
        // Given
        let mut numbers = ElementNumbers::default();
        numbers.set(1, vec![4]);

        // When
        let json = serde_json::to_string(&numbers).expect("serializes");
        let restored: ElementNumbers = serde_json::from_str(&json).expect("deserializes");

        // Then
        assert_eq!(restored, numbers);
    }

    #[test]
    fn test_numbering_step_round_trips_through_json() {
        // Given
        let steps = vec![
            NumberingStep::Element {
                kind: EnumerableKind::CodeBlock,
                sections: vec![SectionId::from_title("Usage")],
            },
            NumberingStep::Toctree {
                position: 0,
                sections: Vec::new(),
            },
        ];

        // When
        let json = serde_json::to_string(&steps).expect("serializes");
        let restored: Vec<NumberingStep> = serde_json::from_str(&json).expect("deserializes");

        // Then
        assert_eq!(restored, steps);
    }

    #[test]
    fn test_subject_kind_is_section_for_a_section() {
        // Given / When / Then
        assert_eq!(
            NumrefSubject::Section(SectionId::from_title("Usage")).kind(),
            EnumerableKind::Section
        );
        assert_eq!(
            NumrefSubject::Element {
                ordinal: 0,
                kind: EnumerableKind::Table
            }
            .kind(),
            EnumerableKind::Table
        );
    }

    #[test]
    fn test_join_number_separates_components_with_dots() {
        // Given / When / Then
        assert_eq!(join_number(&[1, 2, 3]), "1.2.3");
        assert_eq!(join_number(&[4]), "4");
    }
}
