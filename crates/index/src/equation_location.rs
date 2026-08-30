use serde::{Deserialize, Serialize};

/// Where a labeled `.. math::` lives, and which number it was given.
///
/// Unlike [`crate::TargetLocation`], this carries a payload beyond the
/// location: an `:eq:` reference renders the equation's *number* as its link
/// text, so the number has to be resolved centrally — the referencing document
/// cannot count the equations of the document it points into.
///
/// Numbers are assigned per document, starting at 1, and only to equations
/// that carry a label. That is Sphinx's default behaviour (`math_number_all`
/// off, `math_numfig` off); the section-scoped `(1.2)` form is not supported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquationLocation {
    pub doc_path: String,
    pub number: usize,
}

impl EquationLocation {
    #[must_use]
    pub fn new(doc_path: impl Into<String>, number: usize) -> Self {
        Self {
            doc_path: doc_path.into(),
            number,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_records_the_document_and_number() {
        // Given / When
        let location = EquationLocation::new("guide/math.rst", 3);

        // Then
        assert_eq!(location.doc_path, "guide/math.rst");
        assert_eq!(location.number, 3);
    }

    #[test]
    fn test_serialization_roundtrip() {
        // Given
        let location = EquationLocation::new("math.rst", 1);

        // When
        let json = serde_json::to_string(&location).expect("Failed to serialize");
        let deserialized: EquationLocation =
            serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(location, deserialized);
    }
}
