use rinx_ast::SectionId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The `:numbered:` section numbers assigned within one document.
///
/// Keyed by section id, with the document's *own* number stored under the
/// reserved empty key — Sphinx's `secnumbers[docname]['']` convention. The
/// document's number plays the same role as any section's, so it lives in the
/// same collection and is reached through the named [`Self::document`]
/// accessor rather than being split into a separate field.
///
/// Nested maps, rather than one map keyed by `(docname, section)`, for a
/// mechanical reason: `serde_json` requires string map keys, so a tuple key
/// would not round-trip through a `.project.index` file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentNumbers {
    /// Section id → number components. The empty-id entry is the document's
    /// own number; use [`Self::document`] rather than spelling it out.
    numbers: BTreeMap<String, Vec<usize>>,
    /// `.. sectnum::`'s `:prefix:`/`:suffix:`, wrapped around every number
    /// this document's numbers format to. Always empty for numbers assigned
    /// by a `:numbered:` toctree instead, which has no such options.
    #[serde(default)]
    prefix: String,
    #[serde(default)]
    suffix: String,
}

/// The reserved key under which a document's own number is stored.
const DOCUMENT_KEY: &str = "";

impl DocumentNumbers {
    /// The document's own number, as shown beside its title in a toctree.
    #[must_use]
    pub fn document(&self) -> Option<&[usize]> {
        self.numbers.get(DOCUMENT_KEY).map(Vec::as_slice)
    }

    /// One section's number.
    #[must_use]
    pub fn section(&self, id: &SectionId) -> Option<&[usize]> {
        self.numbers.get(id.as_str()).map(Vec::as_slice)
    }

    /// Records the document's own number.
    pub fn set_document(&mut self, number: Vec<usize>) {
        self.numbers.insert(DOCUMENT_KEY.to_string(), number);
    }

    /// Records one section's number.
    pub fn set_section(&mut self, id: &SectionId, number: Vec<usize>) {
        self.numbers.insert(id.as_str().to_string(), number);
    }

    /// Whether nothing in this document is numbered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.numbers.is_empty()
    }

    /// Records `.. sectnum::`'s `:prefix:`/`:suffix:`, so every number
    /// rendered for this document wraps them in.
    pub fn set_format(&mut self, prefix: String, suffix: String) {
        self.prefix = prefix;
        self.suffix = suffix;
    }

    /// The `:prefix:` to prepend to every rendered number. Empty for numbers
    /// assigned by a `:numbered:` toctree.
    #[must_use]
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// The `:suffix:` to append to every rendered number. Empty for numbers
    /// assigned by a `:numbered:` toctree.
    #[must_use]
    pub fn suffix(&self) -> &str {
        &self.suffix
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_number_is_separate_from_a_section_number() {
        // Given
        let mut numbers = DocumentNumbers::default();
        numbers.set_document(vec![2]);
        numbers.set_section(&SectionId::from_title("Install"), vec![2, 1]);

        // When / Then
        assert_eq!(numbers.document(), Some([2].as_slice()));
        assert_eq!(
            numbers.section(&SectionId::from_title("Install")),
            Some([2, 1].as_slice())
        );
    }

    #[test]
    fn test_an_unnumbered_section_has_no_number() {
        // Given
        let numbers = DocumentNumbers::default();

        // When / Then
        assert_eq!(numbers.section(&SectionId::from_title("Install")), None);
        assert_eq!(numbers.document(), None);
        assert!(numbers.is_empty());
    }

    #[test]
    fn test_a_section_cannot_collide_with_the_document_key() {
        // Given — the document key is the empty string, which no `SectionId`
        // can be: `SectionId` rejects an empty value on construction.
        let mut numbers = DocumentNumbers::default();
        numbers.set_document(vec![1]);
        numbers.set_section(&SectionId::from_title("Overview"), vec![1, 1]);

        // When / Then
        assert_eq!(numbers.document(), Some([1].as_slice()));
    }

    #[test]
    fn test_document_numbers_round_trip_through_json() {
        // Given
        let mut numbers = DocumentNumbers::default();
        numbers.set_document(vec![3]);
        numbers.set_section(&SectionId::from_title("Deep"), vec![3, 2, 1]);

        // When
        let json = serde_json::to_string(&numbers).expect("serializes");
        let restored: DocumentNumbers = serde_json::from_str(&json).expect("deserializes");

        // Then
        assert_eq!(restored, numbers);
    }

    #[test]
    fn test_prefix_and_suffix_default_to_empty() {
        // Given
        let numbers = DocumentNumbers::default();

        // When / Then
        assert_eq!(numbers.prefix(), "");
        assert_eq!(numbers.suffix(), "");
    }

    #[test]
    fn test_set_format_records_prefix_and_suffix() {
        // Given
        let mut numbers = DocumentNumbers::default();

        // When
        numbers.set_format("Appendix ".to_string(), ".".to_string());

        // Then
        assert_eq!(numbers.prefix(), "Appendix ");
        assert_eq!(numbers.suffix(), ".");
    }

    #[test]
    fn test_format_round_trips_through_json_alongside_numbers() {
        // Given
        let mut numbers = DocumentNumbers::default();
        numbers.set_document(vec![1]);
        numbers.set_format("Sec ".to_string(), ")".to_string());

        // When
        let json = serde_json::to_string(&numbers).expect("serializes");
        let restored: DocumentNumbers = serde_json::from_str(&json).expect("deserializes");

        // Then
        assert_eq!(restored, numbers);
    }

    #[test]
    fn test_deserializes_when_format_fields_are_missing() {
        // Given — a `.project.index` written before `:prefix:`/`:suffix:`
        // existed.
        let json = r#"{"numbers": {"": [1]}}"#;

        // When
        let numbers: DocumentNumbers = serde_json::from_str(json).expect("deserializes");

        // Then
        assert_eq!(numbers.document(), Some([1].as_slice()));
        assert_eq!(numbers.prefix(), "");
        assert_eq!(numbers.suffix(), "");
    }
}
